use std::collections::BTreeSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_material_bank, parse_overlay, parse_terrain_bank, terrain_blend, ByteSpan,
    Error as FormatError, MaterialError, MaterialPalette8, OverlayKey, Preload, TerrainEntry,
    TerrainError, MATERIAL_HANDLER, TERRAIN_GRID_SIDE, TERRAIN_HANDLER,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

const CHANNELS_PER_ENTRY: usize = 12;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerrainExportOptions {
    pub ovl_filters: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainExportReport {
    pub sources: usize,
    pub entries: usize,
    pub cells: usize,
    pub sparse_records: usize,
    pub pngs: usize,
    pub material_previews: usize,
    pub raw_payload_files: usize,
}

#[derive(Debug, Error)]
pub enum TerrainExportError {
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

    #[error("failed to decode Handler-12 terrain in {path}: {source}")]
    Terrain { path: PathBuf, source: TerrainError },

    #[error("failed to decode Handler-7 materials in {path}: {source}")]
    Material {
        path: PathBuf,
        source: MaterialError,
    },

    #[error("invalid --ovl filter {filter:?}: {source}")]
    InvalidOverlayFilter { filter: String, source: FormatError },

    #[error("no installed OVL matches these --ovl filters: {filters:?}")]
    UnknownOverlayFilters { filters: Vec<String> },

    #[error("requested OVL {path} has no Handler-12 terrain")]
    FilteredOverlayHasNoTerrain { path: PathBuf },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler {handler} but its framed OVL has no such region")]
    MissingRegion { path: PathBuf, handler: u8 },

    #[error("{path} Handler {handler} local slot {local_slot} has no global mapping")]
    MissingGlobalMapping {
        path: PathBuf,
        handler: u8,
        local_slot: usize,
    },

    #[error("source path cannot be represented in the manifest: {0}")]
    InvalidSourcePath(PathBuf),

    #[error("Handler-12 payload span {span:?} in {path} is outside the source OVL")]
    PayloadOutOfBounds { path: PathBuf, span: ByteSpan },

    #[error("failed to encode {path} as PNG: {source}")]
    Png {
        path: PathBuf,
        source: png::EncodingError,
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
struct TerrainManifest {
    schema_version: u32,
    kind: &'static str,
    grid: GridDescription,
    channels: Vec<ChannelDescription>,
    ovl_filters: Vec<String>,
    totals: TerrainTotals,
    sources: Vec<TerrainSource>,
}

#[derive(Serialize)]
struct GridDescription {
    width: usize,
    height: usize,
    png_coordinate_mapping: &'static str,
    source_cell_index: &'static str,
    blend_corner_order_low_to_high: [&'static str; 4],
    edge_addressing: &'static str,
    blend_runtime_provenance: &'static str,
    height_scope: &'static str,
}

#[derive(Serialize)]
struct ChannelDescription {
    key: &'static str,
    color_type: &'static str,
    bit_depth: u8,
    sample: &'static str,
}

#[derive(Default, Serialize)]
struct TerrainTotals {
    source_ovls: usize,
    entries: usize,
    cells: usize,
    sparse_records: usize,
    pngs: usize,
    material_previews: usize,
    raw_payload_files: usize,
    raw_payload_bytes: usize,
}

#[derive(Serialize)]
struct TerrainSource {
    ovl: String,
    level: u8,
    handler_12: HandlerPayloadRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    handler_7_material_palette: Option<MaterialPaletteRecord>,
    entries: Vec<TerrainEntryRecord>,
}

#[derive(Serialize)]
struct HandlerPayloadRecord {
    region_span: ManifestSpan,
    payload_span: ManifestSpan,
    requested_entries: u32,
    raw_payload_file: String,
}

#[derive(Serialize)]
struct MaterialPaletteRecord {
    region_span: ManifestSpan,
    payload_span: ManifestSpan,
    mapping: &'static str,
    preview: &'static str,
    entries: Vec<MaterialValueRecord>,
}

#[derive(Serialize)]
struct MaterialValueRecord {
    local_slot: usize,
    global_slot: u16,
    role: &'static str,
    raw_u32: u32,
    high_word: u16,
    rgb555: u16,
    red5: u8,
    green5: u8,
    blue5: u8,
}

#[derive(Serialize)]
struct TerrainEntryRecord {
    local_slot: usize,
    global_slot: u16,
    source_span: ManifestSpan,
    prefix_span: ManifestSpan,
    prefix: PrefixRecord,
    cells_span: ManifestSpan,
    bucket_sizes_span: ManifestSpan,
    pointer_placeholders_span: ManifestSpan,
    pointer_placeholders: Vec<u32>,
    sparse_records_span: ManifestSpan,
    buckets: Vec<BucketRecord>,
    channels: EntryChannels,
}

#[derive(Serialize)]
struct PrefixRecord {
    plane_height_fixed: i32,
    light_direction_raw: [i32; 3],
    unknown_10: u32,
    interpretation: &'static str,
}

#[derive(Serialize)]
struct BucketRecord {
    y: u16,
    byte_size: u16,
    records_span: ManifestSpan,
    records: Vec<SparseRecord>,
}

#[derive(Serialize)]
struct SparseRecord {
    x: u16,
    state_raw: u8,
    state_low_nibble: u8,
    state_high_nibble: u8,
    resource_lookup_raw: u8,
    resource_lookup_is_special: bool,
    resource_lookup_interpretation: &'static str,
}

#[derive(Serialize)]
struct EntryChannels {
    height_bias_128: String,
    material_id: String,
    shade_nibble: String,
    sparse_overlay_mask: String,
    stored_alternate_diagonal: String,
    blend_source_combo: String,
    blend_canonical_combo: String,
    blend_texture_rank: String,
    blend_texture_resource_id: String,
    blend_symmetry: String,
    blend_orientation_word_le: String,
    derived_alternate_diagonal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    material_color_preview: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
struct ManifestSpan {
    start: usize,
    end: usize,
    byte_len: usize,
}

impl From<ByteSpan> for ManifestSpan {
    fn from(span: ByteSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
            byte_len: span.len(),
        }
    }
}

struct ResolvedMaterialPalette {
    palette: MaterialPalette8,
    record: MaterialPaletteRecord,
}

struct ChannelData {
    height_bias_128: Vec<u8>,
    material_id: Vec<u8>,
    shade_nibble: Vec<u8>,
    sparse_overlay_mask: Vec<u8>,
    stored_alternate_diagonal: Vec<u8>,
    blend_source_combo: Vec<u8>,
    blend_canonical_combo: Vec<u8>,
    blend_texture_rank: Vec<u8>,
    blend_texture_resource_id_be: Vec<u8>,
    blend_symmetry: Vec<u8>,
    blend_orientation_word_le: Vec<u8>,
    derived_alternate_diagonal: Vec<u8>,
}

pub fn export(
    data_root: &Path,
    output_root: &Path,
    options: &TerrainExportOptions,
) -> Result<TerrainExportReport, TerrainExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| TerrainExportError::Format {
        path: preload_path,
        source,
    })?;

    let target = output_root.join("terrain");
    let staged = StagedOutput::new(&target)?;
    create_dir_all(&staged.path().join("sources"))?;

    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| TerrainExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let (selected_paths, selected_filter_names, filtering) =
        select_overlay_paths(overlay_paths, &options.ovl_filters)?;

    let mut totals = TerrainTotals::default();
    let mut sources = Vec::new();
    for path in selected_paths {
        let Some(source) = export_source(&preload, &path, staged.path(), filtering, &mut totals)?
        else {
            continue;
        };
        sources.push(source);
    }

    let report = TerrainExportReport {
        sources: totals.source_ovls,
        entries: totals.entries,
        cells: totals.cells,
        sparse_records: totals.sparse_records,
        pngs: totals.pngs,
        material_previews: totals.material_previews,
        raw_payload_files: totals.raw_payload_files,
    };
    let manifest = TerrainManifest {
        schema_version: 1,
        kind: "infestation_handler_12_terrain",
        grid: grid_description(),
        channels: channel_descriptions(),
        ovl_filters: selected_filter_names,
        totals,
        sources,
    };
    write_manifest(staged.path(), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn export_source(
    preload: &Preload,
    path: &Path,
    staged_root: &Path,
    filtering: bool,
    totals: &mut TerrainTotals,
) -> Result<Option<TerrainSource>, TerrainExportError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| TerrainExportError::InvalidSourcePath(path.to_owned()))?;
    let key = OverlayKey::parse_filename(name).map_err(|source| TerrainExportError::Format {
        path: path.to_owned(),
        source,
    })?;
    let leaf = preload
        .leaf(key.level)
        .ok_or_else(|| TerrainExportError::MissingLeaf {
            path: path.to_owned(),
            level: key.level,
        })?;
    let requests = leaf.requests(key.request_kind());
    let terrain_request = requests
        .get(usize::from(TERRAIN_HANDLER))
        .expect("PreLoad has exactly 16 handlers");
    if terrain_request.count == 0 {
        if filtering {
            return Err(TerrainExportError::FilteredOverlayHasNoTerrain {
                path: path.to_owned(),
            });
        }
        return Ok(None);
    }

    let source = read(path)?;
    let layout = parse_overlay(&source, requests).map_err(|source| TerrainExportError::Format {
        path: path.to_owned(),
        source,
    })?;
    let terrain_region = layout
        .regions
        .iter()
        .find(|region| region.handler.get() == TERRAIN_HANDLER)
        .ok_or_else(|| TerrainExportError::MissingRegion {
            path: path.to_owned(),
            handler: TERRAIN_HANDLER,
        })?;
    let terrain_bank = parse_terrain_bank(&source, terrain_region).map_err(|source| {
        TerrainExportError::Terrain {
            path: path.to_owned(),
            source,
        }
    })?;
    let materials = resolve_material_palette(path, &source, &layout.regions, requests)?;

    let source_key = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| TerrainExportError::InvalidSourcePath(path.to_owned()))?;
    let source_dir = format!("sources/{source_key}");
    create_dir_all(&staged_root.join(Path::new(&source_dir)))?;
    let raw_payload_file = format!("{source_dir}/handler_12_payload.bin");
    let raw_payload = terrain_bank.payload.get(&source).ok_or_else(|| {
        TerrainExportError::PayloadOutOfBounds {
            path: path.to_owned(),
            span: terrain_bank.payload,
        }
    })?;
    write(&staged_root.join(Path::new(&raw_payload_file)), raw_payload)?;
    totals.raw_payload_files += 1;
    totals.raw_payload_bytes += raw_payload.len();

    let mut entries = Vec::with_capacity(terrain_bank.entries.len());
    for (local_slot, entry) in terrain_bank.entries.iter().enumerate() {
        let global_slot = terrain_request
            .global_index(local_slot as u16)
            .ok_or_else(|| TerrainExportError::MissingGlobalMapping {
                path: path.to_owned(),
                handler: TERRAIN_HANDLER,
                local_slot,
            })?;
        let relative_dir = format!("{source_dir}/entry_{local_slot:02}_global_{global_slot:04}");
        let channels = write_entry_channels(
            staged_root,
            &relative_dir,
            entry,
            materials.as_ref().map(|resolved| resolved.palette),
        )?;
        let buckets = entry
            .buckets()
            .iter()
            .map(|bucket| BucketRecord {
                y: bucket.y,
                byte_size: bucket.byte_size,
                records_span: bucket.records_span.into(),
                records: bucket
                    .records
                    .iter()
                    .copied()
                    .map(|record| SparseRecord {
                        x: record.x,
                        state_raw: record.state,
                        state_low_nibble: record.state_low_nibble(),
                        state_high_nibble: record.state_high_nibble(),
                        resource_lookup_raw: record.resource_lookup.raw(),
                        resource_lookup_is_special: record.resource_lookup.is_special(),
                        resource_lookup_interpretation:
                            "raw index into the separately loaded sparse-resource lookup; not a global resource ID",
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        let sparse_records = buckets
            .iter()
            .map(|bucket| bucket.records.len())
            .sum::<usize>();

        totals.entries += 1;
        totals.cells += entry.cells().len();
        totals.sparse_records += sparse_records;
        totals.pngs += CHANNELS_PER_ENTRY;
        if channels.material_color_preview.is_some() {
            totals.pngs += 1;
            totals.material_previews += 1;
        }
        entries.push(TerrainEntryRecord {
            local_slot,
            global_slot,
            source_span: entry.span.into(),
            prefix_span: entry.prefix_span.into(),
            prefix: PrefixRecord {
                plane_height_fixed: entry.prefix.plane_height_fixed,
                light_direction_raw: entry.prefix.light_direction_raw,
                unknown_10: entry.prefix.unknown_10,
                interpretation: "raw Handler-12 prefix; no scene-supplied height shift or water/plane policy has been guessed",
            },
            cells_span: entry.cells_span.into(),
            bucket_sizes_span: entry.bucket_sizes_span.into(),
            pointer_placeholders_span: entry.pointer_placeholders_span.into(),
            pointer_placeholders: entry.pointer_placeholders().to_vec(),
            sparse_records_span: entry.sparse_records_span.into(),
            buckets,
            channels,
        });
    }

    totals.source_ovls += 1;
    Ok(Some(TerrainSource {
        ovl: name.to_owned(),
        level: key.level,
        handler_12: HandlerPayloadRecord {
            region_span: terrain_region.span.into(),
            payload_span: terrain_bank.payload.into(),
            requested_entries: terrain_region.requested_count,
            raw_payload_file,
        },
        handler_7_material_palette: materials.map(|resolved| resolved.record),
        entries,
    }))
}

fn resolve_material_palette(
    path: &Path,
    source: &[u8],
    regions: &[inf_formats::HandlerRegion],
    requests: &inf_formats::RequestTable,
) -> Result<Option<ResolvedMaterialPalette>, TerrainExportError> {
    let request = requests
        .get(usize::from(MATERIAL_HANDLER))
        .expect("PreLoad has exactly 16 handlers");
    if request.count != 8 {
        return Ok(None);
    }
    let region = regions
        .iter()
        .find(|region| region.handler.get() == MATERIAL_HANDLER)
        .ok_or_else(|| TerrainExportError::MissingRegion {
            path: path.to_owned(),
            handler: MATERIAL_HANDLER,
        })?;
    let bank =
        parse_material_bank(source, region).map_err(|source| TerrainExportError::Material {
            path: path.to_owned(),
            source,
        })?;
    let Some(palette) = bank.palette8() else {
        return Ok(None);
    };
    let mut entries = Vec::with_capacity(palette.entries().len());
    for (local_slot, value) in palette.entries().iter().copied().enumerate() {
        let global_slot = request.global_index(local_slot as u16).ok_or_else(|| {
            TerrainExportError::MissingGlobalMapping {
                path: path.to_owned(),
                handler: MATERIAL_HANDLER,
                local_slot,
            }
        })?;
        let rgb = value.rgb555();
        entries.push(MaterialValueRecord {
            local_slot,
            global_slot,
            role: match local_slot {
                0..=3 => "terrain_surface_selected_by_same_numbered_H12_material_id",
                4 => "recovered_plane_or_water_material",
                _ => "unresolved_handler_7_terrain_level_entry",
            },
            raw_u32: value.raw(),
            high_word: value.high_word(),
            rgb555: rgb.raw(),
            red5: rgb.red5(),
            green5: rgb.green5(),
            blue5: rgb.blue5(),
        });
    }
    Ok(Some(ResolvedMaterialPalette {
        palette,
        record: MaterialPaletteRecord {
            region_span: region.span.into(),
            payload_span: bank.payload.into(),
            mapping: "H12 cell material IDs 0 through 3 select same-numbered H7 entries; H7 entry 4 is not used by this surface preview",
            preview: "unshaded RGB8 expansion of the recovered RGB555 surface material only; not a textured terrain reconstruction",
            entries,
        },
    }))
}

fn write_entry_channels(
    staged_root: &Path,
    relative_dir: &str,
    entry: &TerrainEntry,
    material_palette: Option<MaterialPalette8>,
) -> Result<EntryChannels, TerrainExportError> {
    create_dir_all(&staged_root.join(Path::new(relative_dir)))?;
    let data = build_channel_data(entry);
    let height_bias_128 = format!("{relative_dir}/height_bias_128.png");
    let material_id = format!("{relative_dir}/material_id.png");
    let shade_nibble = format!("{relative_dir}/shade_nibble.png");
    let sparse_overlay_mask = format!("{relative_dir}/sparse_overlay_mask.png");
    let stored_alternate_diagonal = format!("{relative_dir}/stored_alternate_diagonal.png");
    let blend_source_combo = format!("{relative_dir}/blend_source_combo.png");
    let blend_canonical_combo = format!("{relative_dir}/blend_canonical_combo.png");
    let blend_texture_rank = format!("{relative_dir}/blend_texture_rank.png");
    let blend_texture_resource_id = format!("{relative_dir}/blend_texture_resource_id.png");
    let blend_symmetry = format!("{relative_dir}/blend_symmetry.png");
    let blend_orientation_word_le = format!("{relative_dir}/blend_orientation_word_le.rgba.png");
    let derived_alternate_diagonal = format!("{relative_dir}/derived_alternate_diagonal.png");

    for (file, pixels) in [
        (&height_bias_128, &data.height_bias_128),
        (&material_id, &data.material_id),
        (&shade_nibble, &data.shade_nibble),
        (&sparse_overlay_mask, &data.sparse_overlay_mask),
        (&stored_alternate_diagonal, &data.stored_alternate_diagonal),
        (&blend_source_combo, &data.blend_source_combo),
        (&blend_canonical_combo, &data.blend_canonical_combo),
        (&blend_texture_rank, &data.blend_texture_rank),
        (&blend_symmetry, &data.blend_symmetry),
        (
            &derived_alternate_diagonal,
            &data.derived_alternate_diagonal,
        ),
    ] {
        write_png(
            staged_root,
            file,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            pixels,
        )?;
    }
    write_png(
        staged_root,
        &blend_texture_resource_id,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &data.blend_texture_resource_id_be,
    )?;
    write_png(
        staged_root,
        &blend_orientation_word_le,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &data.blend_orientation_word_le,
    )?;
    let material_color_preview = material_palette
        .map(|palette| {
            let file = format!("{relative_dir}/material_color_preview.png");
            let rgb = material_preview(entry, palette);
            write_png(
                staged_root,
                &file,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &rgb,
            )?;
            Ok::<_, TerrainExportError>(file)
        })
        .transpose()?;

    Ok(EntryChannels {
        height_bias_128,
        material_id,
        shade_nibble,
        sparse_overlay_mask,
        stored_alternate_diagonal,
        blend_source_combo,
        blend_canonical_combo,
        blend_texture_rank,
        blend_texture_resource_id,
        blend_symmetry,
        blend_orientation_word_le,
        derived_alternate_diagonal,
        material_color_preview,
    })
}

fn build_channel_data(entry: &TerrainEntry) -> ChannelData {
    let cell_count = TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE;
    let mut data = ChannelData {
        height_bias_128: Vec::with_capacity(cell_count),
        material_id: Vec::with_capacity(cell_count),
        shade_nibble: Vec::with_capacity(cell_count),
        sparse_overlay_mask: Vec::with_capacity(cell_count),
        stored_alternate_diagonal: Vec::with_capacity(cell_count),
        blend_source_combo: Vec::with_capacity(cell_count),
        blend_canonical_combo: Vec::with_capacity(cell_count),
        blend_texture_rank: Vec::with_capacity(cell_count),
        blend_texture_resource_id_be: Vec::with_capacity(cell_count * 2),
        blend_symmetry: Vec::with_capacity(cell_count),
        blend_orientation_word_le: Vec::with_capacity(cell_count * 4),
        derived_alternate_diagonal: Vec::with_capacity(cell_count),
    };

    for y in 0..TERRAIN_GRID_SIDE {
        let next_y = (y + 1) % TERRAIN_GRID_SIDE;
        for x in 0..TERRAIN_GRID_SIDE {
            let next_x = (x + 1) % TERRAIN_GRID_SIDE;
            let cell = entry.cell(x, y).expect("bounded terrain coordinate");
            let combo = pack_material_combo(
                cell.material(),
                entry
                    .cell(x, next_y)
                    .expect("wrapped terrain coordinate")
                    .material(),
                entry
                    .cell(next_x, y)
                    .expect("wrapped terrain coordinate")
                    .material(),
                entry
                    .cell(next_x, next_y)
                    .expect("wrapped terrain coordinate")
                    .material(),
            );
            let blend = terrain_blend(combo);

            data.height_bias_128
                .push((i16::from(cell.height_i8()) + 128) as u8);
            data.material_id.push(cell.material());
            data.shade_nibble.push(cell.shade4());
            data.sparse_overlay_mask
                .push(bool_mask(cell.has_sparse_record()));
            data.stored_alternate_diagonal
                .push(bool_mask(cell.stored_alternate_diagonal()));
            data.blend_source_combo.push(combo);
            data.blend_canonical_combo.push(blend.canonical_combo);
            data.blend_texture_rank.push(blend.texture_rank);
            data.blend_texture_resource_id_be
                .extend_from_slice(&blend.texture_resource_id.to_be_bytes());
            data.blend_symmetry.push(blend.symmetry as u8);
            data.blend_orientation_word_le
                .extend_from_slice(&blend.orientation_word.to_le_bytes());
            data.derived_alternate_diagonal
                .push(bool_mask(blend.uses_alternate_diagonal()));
        }
    }
    data
}

const fn pack_material_combo(m00: u8, m01: u8, m10: u8, m11: u8) -> u8 {
    m00 | (m01 << 2) | (m10 << 4) | (m11 << 6)
}

const fn bool_mask(value: bool) -> u8 {
    if value {
        255
    } else {
        0
    }
}

fn material_preview(entry: &TerrainEntry, palette: MaterialPalette8) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE * 3);
    for y in 0..TERRAIN_GRID_SIDE {
        for x in 0..TERRAIN_GRID_SIDE {
            let material = usize::from(
                entry
                    .cell(x, y)
                    .expect("bounded terrain coordinate")
                    .material(),
            );
            let value = palette.entries()[material].rgb555();
            rgb.extend_from_slice(&[
                expand_5_to_8(value.red5()),
                expand_5_to_8(value.green5()),
                expand_5_to_8(value.blue5()),
            ]);
        }
    }
    rgb
}

const fn expand_5_to_8(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn select_overlay_paths(
    overlay_paths: Vec<PathBuf>,
    filters: &[String],
) -> Result<(Vec<PathBuf>, Vec<String>, bool), TerrainExportError> {
    if filters.is_empty() {
        return Ok((overlay_paths, Vec::new(), false));
    }

    let mut requested = BTreeSet::new();
    for filter in filters {
        OverlayKey::parse_filename(filter).map_err(|source| {
            TerrainExportError::InvalidOverlayFilter {
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
        return Err(TerrainExportError::UnknownOverlayFilters {
            filters: unmatched.into_iter().collect(),
        });
    }
    Ok((selected, selected_names, true))
}

fn grid_description() -> GridDescription {
    GridDescription {
        width: TERRAIN_GRID_SIDE,
        height: TERRAIN_GRID_SIDE,
        png_coordinate_mapping: "PNG pixel (x,y) is TerrainEntry::cell(x,y)",
        source_cell_index: "x * 512 + y (the executable's Handler-12 storage order)",
        blend_corner_order_low_to_high: ["(x,y)", "(x,y+1)", "(x+1,y)", "(x+1,y+1)"],
        edge_addressing: "native 512-grid coordinates wrap modulo 512, matching FUN_004506A0 with DAT_005372F0=0x80 and DAT_005372F4=0x7f",
        blend_runtime_provenance: "source combo packing and derived diagonal reproduce FUN_004506A0; canonical texture rank/resource and orientation reproduce FUN_0044B5C0",
        height_scope: "height_bias_128 preserves only the signed on-disk i8 cell height; no scene height shift, plane threshold conversion, water level, or render mesh is inferred",
    }
}

fn channel_descriptions() -> Vec<ChannelDescription> {
    vec![
        channel("height_bias_128", "grayscale", 8, "signed i8 height + 128"),
        channel("material_id", "grayscale", 8, "raw two-bit material ID 0..3"),
        channel("shade_nibble", "grayscale", 8, "raw high-nibble shade value 0..15"),
        channel("sparse_overlay_mask", "grayscale", 8, "0=false, 255=cell bit 10 is set"),
        channel("stored_alternate_diagonal", "grayscale", 8, "0=false, 255=on-disk cell bit 11 is set"),
        channel("blend_source_combo", "grayscale", 8, "four two-bit corner material IDs packed low-to-high in documented order"),
        channel("blend_canonical_combo", "grayscale", 8, "canonical representative from the executable's eight symmetries"),
        channel("blend_texture_rank", "grayscale", 8, "exact rank 0..54 in the recovered 55-texture table"),
        channel("blend_texture_resource_id", "grayscale", 16, "exact unsigned resource ID 0x04fc..0x0532; PNG samples are network-byte-order u16"),
        channel("blend_symmetry", "grayscale", 8, "TerrainBlendSymmetry discriminant 0..7"),
        channel("blend_orientation_word_le", "rgba", 8, "exact u32 orientation word packed R=bits0..7, G=8..15, B=16..23, A=24..31; diagnostic data, not a color preview"),
        channel("derived_alternate_diagonal", "grayscale", 8, "0=false, 255=FUN_004506A0 orientation low nibble is neither 0 nor 2"),
        channel("material_color_preview", "rgb", 8, "optional unshaded RGB555 expansion from same-OVL Handler 7 entries 0..3; omitted without the exact eight-entry palette"),
    ]
}

const fn channel(
    key: &'static str,
    color_type: &'static str,
    bit_depth: u8,
    sample: &'static str,
) -> ChannelDescription {
    ChannelDescription {
        key,
        color_type,
        bit_depth,
        sample,
    }
}

fn write_png(
    staged_root: &Path,
    relative_path: &str,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    pixels: &[u8],
) -> Result<(), TerrainExportError> {
    let path = staged_root.join(Path::new(relative_path));
    let file = fs::File::create(&path).map_err(|source| TerrainExportError::Write {
        path: path.clone(),
        source,
    })?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        TERRAIN_GRID_SIDE as u32,
        TERRAIN_GRID_SIDE as u32,
    );
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder
        .write_header()
        .map_err(|source| TerrainExportError::Png {
            path: path.clone(),
            source,
        })?;
    writer
        .write_image_data(pixels)
        .map_err(|source| TerrainExportError::Png { path, source })?;
    Ok(())
}

fn write_manifest(root: &Path, manifest: &TerrainManifest) -> Result<(), TerrainExportError> {
    let path = root.join("manifest.json");
    let file = fs::File::create(&path).map_err(|source| TerrainExportError::Write {
        path: path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, manifest).map_err(|source| {
        TerrainExportError::Json {
            path: path.clone(),
            source,
        }
    })?;
    writer
        .flush()
        .map_err(|source| TerrainExportError::Write { path, source })?;
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, TerrainExportError> {
    fs::read(path).map_err(|source| TerrainExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), TerrainExportError> {
    fs::create_dir_all(path).map_err(|source| TerrainExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), TerrainExportError> {
    fs::write(path, bytes).map_err(|source| TerrainExportError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{parse_overlay, parse_terrain_bank, TerrainBlendSymmetry};

    #[test]
    fn exports_three_terrain_levels_with_exact_channels_and_h7_previews() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root = temp_output("terrain-export");
        let _ = fs::remove_dir_all(&output_root);
        let options = TerrainExportOptions {
            ovl_filters: vec![
                "35.ovl".to_owned(),
                "11.OVL".to_owned(),
                "29.ovl".to_owned(),
                "11.ovl".to_owned(),
            ],
        };

        let first = export(&data_root, &output_root, &options).unwrap();
        assert_eq!(
            first,
            TerrainExportReport {
                sources: 3,
                entries: 3,
                cells: 3 * TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE,
                sparse_records: 218 + 79,
                pngs: 3 * (CHANNELS_PER_ENTRY + 1),
                material_previews: 3,
                raw_payload_files: 3,
            }
        );

        let manifest_path = output_root.join("terrain/manifest.json");
        let first_manifest = fs::read(&manifest_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        assert_eq!(
            manifest["ovl_filters"],
            serde_json::json!(["11.ovl", "29.ovl", "35.ovl"])
        );
        assert_eq!(
            manifest["sources"][0]["entries"][0]["buckets"]
                .as_array()
                .unwrap()
                .len(),
            512
        );
        assert_eq!(
            manifest["sources"][0]["handler_7_material_palette"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            manifest["sources"][0]["entries"][0]["prefix"]["interpretation"],
            "raw Handler-12 prefix; no scene-supplied height shift or water/plane policy has been guessed"
        );

        let level_dir = output_root.join("terrain/sources/11/entry_00_global_0000");
        assert_png(
            &level_dir.join("height_bias_128.png"),
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
        );
        assert_png(
            &level_dir.join("blend_texture_resource_id.png"),
            png::ColorType::Grayscale,
            png::BitDepth::Sixteen,
        );
        assert_png(
            &level_dir.join("blend_orientation_word_le.rgba.png"),
            png::ColorType::Rgba,
            png::BitDepth::Eight,
        );
        assert_png(
            &level_dir.join("material_color_preview.png"),
            png::ColorType::Rgb,
            png::BitDepth::Eight,
        );

        assert_level_11_first_pixel_matches_formats_api(&data_root, &output_root);

        let before_png = fs::read(level_dir.join("blend_source_combo.png")).unwrap();
        assert_eq!(export(&data_root, &output_root, &options).unwrap(), first);
        assert_eq!(fs::read(&manifest_path).unwrap(), first_manifest);
        assert_eq!(
            fs::read(
                output_root.join("terrain/sources/11/entry_00_global_0000/blend_source_combo.png")
            )
            .unwrap(),
            before_png
        );
        let _ = fs::remove_dir_all(output_root);
    }

    fn assert_level_11_first_pixel_matches_formats_api(data_root: &Path, output_root: &Path) {
        let preload = Preload::parse(&fs::read(data_root.join("PreLoad.dat")).unwrap()).unwrap();
        let source = fs::read(data_root.join("Overlay/11.ovl")).unwrap();
        let requests = &preload.leaf(11).unwrap().base;
        let layout = parse_overlay(&source, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == TERRAIN_HANDLER)
            .unwrap();
        let bank = parse_terrain_bank(&source, region).unwrap();
        let entry = &bank.entries[0];
        assert_eq!(
            fs::read(output_root.join("terrain/sources/11/handler_12_payload.bin")).unwrap(),
            bank.payload.get(&source).unwrap()
        );

        let cell = entry.cell(0, 0).unwrap();
        let combo = pack_material_combo(
            cell.material(),
            entry.cell(0, 1).unwrap().material(),
            entry.cell(1, 0).unwrap().material(),
            entry.cell(1, 1).unwrap().material(),
        );
        let blend = terrain_blend(combo);
        let base = output_root.join("terrain/sources/11/entry_00_global_0000");
        assert_eq!(
            decode_png(&base.join("height_bias_128.png"))[0],
            (i16::from(cell.height_i8()) + 128) as u8
        );
        assert_eq!(
            decode_png(&base.join("material_id.png"))[0],
            cell.material()
        );
        assert_eq!(decode_png(&base.join("blend_source_combo.png"))[0], combo);
        assert_eq!(
            decode_png(&base.join("blend_canonical_combo.png"))[0],
            blend.canonical_combo
        );
        assert_eq!(
            decode_png(&base.join("blend_texture_rank.png"))[0],
            blend.texture_rank
        );
        assert_eq!(
            &decode_png(&base.join("blend_texture_resource_id.png"))[0..2],
            &blend.texture_resource_id.to_be_bytes()
        );
        assert_eq!(
            &decode_png(&base.join("blend_orientation_word_le.rgba.png"))[0..4],
            &blend.orientation_word.to_le_bytes()
        );
        assert_eq!(
            decode_png(&base.join("derived_alternate_diagonal.png"))[0],
            bool_mask(blend.uses_alternate_diagonal())
        );

        let material_region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == MATERIAL_HANDLER)
            .unwrap();
        let material_bank = parse_material_bank(&source, material_region).unwrap();
        let material =
            material_bank.palette8().unwrap().entries()[usize::from(cell.material())].rgb555();
        assert_eq!(
            &decode_png(&base.join("material_color_preview.png"))[0..3],
            &[
                expand_5_to_8(material.red5()),
                expand_5_to_8(material.green5()),
                expand_5_to_8(material.blue5()),
            ]
        );
    }

    fn assert_png(path: &Path, color_type: png::ColorType, bit_depth: png::BitDepth) {
        let decoder = png::Decoder::new(fs::File::open(path).unwrap());
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (512, 512));
        assert_eq!(reader.info().color_type, color_type);
        assert_eq!(reader.info().bit_depth, bit_depth);
    }

    fn decode_png(path: &Path) -> Vec<u8> {
        let decoder = png::Decoder::new(fs::File::open(path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        pixels
    }

    fn temp_output(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("infestation-{label}-{}", std::process::id()))
    }

    #[test]
    fn symmetry_discriminants_remain_the_manifest_contract() {
        assert_eq!(TerrainBlendSymmetry::Identity as u8, 0);
        assert_eq!(TerrainBlendSymmetry::SwapAdjacentPairs as u8, 7);
    }
}
