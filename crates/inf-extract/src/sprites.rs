use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use inf_formats::{
    parse_overlay, parse_sprite_bank, ByteSpan, Error as FormatError, OverlayKey, Preload, Rgba8,
    SpriteBlendMode, SpriteEncoding, SpriteError, SpriteFlags, SpriteModeSelectors,
    SpriteRenderMode, SpriteShade, SpriteVariant, SPRITE_ATLAS_STRIDE_BYTES, SPRITE_SHADE_LEVELS,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteExportOptions {
    pub shade: SpriteExportShade,
    pub ovl_filters: Vec<String>,
}

impl Default for SpriteExportOptions {
    fn default() -> Self {
        Self {
            shade: SpriteExportShade::Base,
            ovl_filters: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpriteExportShade {
    #[default]
    Base,
    Level(u8),
}

impl SpriteExportShade {
    fn decoder_shade(self) -> SpriteShade {
        match self {
            Self::Base => SpriteShade::Base,
            Self::Level(level) => SpriteShade::Level(level),
        }
    }
}

impl fmt::Display for SpriteExportShade {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base => formatter.write_str("base"),
            Self::Level(level) => write!(formatter, "{level}"),
        }
    }
}

impl FromStr for SpriteExportShade {
    type Err = SpriteExportShadeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("base") {
            return Ok(Self::Base);
        }
        let level = value
            .parse::<u8>()
            .map_err(|_| SpriteExportShadeError(value.to_owned()))?;
        if level >= SPRITE_SHADE_LEVELS {
            return Err(SpriteExportShadeError(value.to_owned()));
        }
        Ok(Self::Level(level))
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("expected `base` or a shade level from 0 through 31, received {0:?}")]
pub struct SpriteExportShadeError(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteExportReport {
    pub sources: usize,
    pub raw_region_files: usize,
    pub registered_entries: usize,
    pub orphan_entries: usize,
    pub physical_variants: usize,
    pub pngs: usize,
    pub raw_pixel_files: usize,
    pub unique_palette_files: usize,
}

#[derive(Debug, Error)]
pub enum SpriteExportError {
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

    #[error("failed to decode sprites in {path}: {source}")]
    Sprite { path: PathBuf, source: SpriteError },

    #[error("invalid --ovl filter {filter:?}: {source}")]
    InvalidOverlayFilter { filter: String, source: FormatError },

    #[error("no installed OVL matches these --ovl filters: {filters:?}")]
    UnknownOverlayFilters { filters: Vec<String> },

    #[error("requested OVL {path} has no Handler-3 sprites")]
    FilteredOverlayHasNoSprites { path: PathBuf },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler 3 but its framed OVL has no Handler-3 region")]
    MissingSpriteRegion { path: PathBuf },

    #[error("{path} Handler-3 local slot {local_slot} has no global resource mapping")]
    MissingGlobalMapping { path: PathBuf, local_slot: u32 },

    #[error("source path cannot be represented in the manifest: {0}")]
    InvalidSourcePath(PathBuf),

    #[error("sprite pixel span {span:?} in {path} is outside the source OVL")]
    PixelSpanOutOfBounds { path: PathBuf, span: ByteSpan },

    #[error("sprite palette span {span:?} in {path} is outside the source OVL")]
    PaletteSpanOutOfBounds { path: PathBuf, span: ByteSpan },

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
struct SpriteManifest {
    schema_version: u32,
    kind: &'static str,
    selected_shade: String,
    atlas_stride_bytes: u32,
    palette_layout: PaletteLayout,
    ovl_filters: Vec<String>,
    totals: SpriteTotals,
    palettes: Vec<PaletteAsset>,
    sources: Vec<SpriteSource>,
}

#[derive(Serialize)]
struct PaletteLayout {
    encoding: &'static str,
    shade_levels: u8,
    organization: &'static str,
    base_colors: &'static str,
}

#[derive(Default, Serialize)]
struct SpriteTotals {
    source_ovls: usize,
    raw_region_files: usize,
    registered_entries: usize,
    orphan_entries: usize,
    physical_variants: usize,
    pngs: usize,
    raw_pixel_files: usize,
    unique_palette_files: usize,
}

#[derive(Serialize)]
struct PaletteAsset {
    file: String,
    byte_len: usize,
}

#[derive(Serialize)]
struct SpriteSource {
    ovl: String,
    framed_region_span: ManifestSpan,
    raw_framed_region: String,
    main_payload_span: ManifestSpan,
    descriptor_table_span: ManifestSpan,
    requested_entries: u32,
    unreferenced_atlas_record_indices: Vec<usize>,
    registered_entries: Vec<RegisteredEntryRecord>,
    orphan_entries: Vec<OrphanEntryRecord>,
}

#[derive(Serialize)]
struct RegisteredEntryRecord {
    local_slot: u32,
    global_slot: u16,
    source_span: ManifestSpan,
    selectors: SelectorRecord,
    variants: Vec<VariantRecord>,
}

#[derive(Serialize)]
struct OrphanEntryRecord {
    orphan_index: usize,
    status: &'static str,
    source_span: ManifestSpan,
    selectors: SelectorRecord,
    variants: Vec<VariantRecord>,
}

#[derive(Serialize)]
struct SelectorRecord {
    mode0_raw: u8,
    mode1_raw: u8,
    mode0_variant: Option<usize>,
    mode1_variant: Option<usize>,
    physical_variant_count: usize,
    reserved: u16,
}

impl From<SpriteModeSelectors> for SelectorRecord {
    fn from(selectors: SpriteModeSelectors) -> Self {
        Self {
            mode0_raw: selectors.mode0,
            mode1_raw: selectors.mode1,
            mode0_variant: selectors.variant_index(SpriteRenderMode::Mode0),
            mode1_variant: selectors.variant_index(SpriteRenderMode::Mode1),
            physical_variant_count: selectors.physical_variant_count(),
            reserved: selectors.reserved,
        }
    }
}

#[derive(Serialize)]
struct VariantRecord {
    physical_index: u8,
    descriptor_global_id: u16,
    descriptor_span: ManifestSpan,
    selected_by_modes: Vec<SpriteRenderMode>,
    flags: FlagRecord,
    width: u16,
    height: u16,
    atlas: AtlasRecord,
    png_file: String,
    raw_pixels_file: String,
    encoding: EncodingRecord,
}

#[derive(Serialize)]
struct FlagRecord {
    raw: u16,
    masked: bool,
    direct_rgb555: bool,
    alpha_blend_bit: bool,
    shared_palette: bool,
    blend_mode: SpriteBlendMode,
}

impl From<SpriteFlags> for FlagRecord {
    fn from(flags: SpriteFlags) -> Self {
        Self {
            raw: flags.raw(),
            masked: flags.is_masked(),
            direct_rgb555: flags.is_direct_rgb555(),
            alpha_blend_bit: flags.uses_alpha_blend(),
            shared_palette: flags.palette_is_shared(),
            blend_mode: flags.blend_mode(),
        }
    }
}

#[derive(Serialize)]
struct AtlasRecord {
    byte_offset: u32,
    x_bytes: u16,
    y: u16,
    atlas_record_index: usize,
    record_span: ManifestSpan,
    pixel_span: ManifestSpan,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EncodingRecord {
    Indexed {
        raw_pixel_format: &'static str,
        color_count: u16,
        palette_offset_in_main_payload: u32,
        palette_file: String,
        palette_span: ManifestSpan,
        shade_ramps_span: ManifestSpan,
        base_colors_span: ManifestSpan,
    },
    DirectRgb555 {
        raw_pixel_format: &'static str,
        ignored_palette_offset_in_main_payload: u32,
    },
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

pub fn export(
    data_root: &Path,
    output_root: &Path,
    options: &SpriteExportOptions,
) -> Result<SpriteExportReport, SpriteExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| SpriteExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("sprites");
    let staged = StagedOutput::new(&target)?;
    create_dir_all(&staged.path().join("sources"))?;
    create_dir_all(&staged.path().join("palettes"))?;

    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| SpriteExportError::Read {
            path: overlay_dir,
            source,
        })?;
    let (selected_paths, selected_filter_names, filtering) =
        select_overlay_paths(overlay_paths, &options.ovl_filters)?;

    let mut totals = SpriteTotals::default();
    let mut sources = Vec::new();
    let mut palettes = PaletteStore::new(staged.path());

    for path in selected_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SpriteExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| SpriteExportError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| SpriteExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let sprite_request = requests.get(3).expect("PreLoad has exactly 16 handlers");
        if sprite_request.count == 0 {
            if filtering {
                return Err(SpriteExportError::FilteredOverlayHasNoSprites { path });
            }
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| SpriteExportError::Format {
                path: path.clone(),
                source,
            })?;
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 3)
            .ok_or_else(|| SpriteExportError::MissingSpriteRegion { path: path.clone() })?;
        let bank =
            parse_sprite_bank(&bytes, region).map_err(|source| SpriteExportError::Sprite {
                path: path.clone(),
                source,
            })?;
        let source_key = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| SpriteExportError::InvalidSourcePath(path.clone()))?;
        let raw_region_relative = format!("sources/{source_key}/handler_03_framed.bin");
        create_dir_all(&staged.path().join("sources").join(source_key))?;
        write(
            &staged.path().join(&raw_region_relative),
            region
                .span
                .get(&bytes)
                .expect("overlay parser validated the Handler-3 region span"),
        )?;
        let (registered_entries, orphan_entries) = {
            let mut variant_exporter = VariantExporter {
                source_path: &path,
                source: &bytes,
                staged_root: staged.path(),
                shade: options.shade,
                bank: &bank,
                palettes: &mut palettes,
            };
            let mut registered_entries = Vec::with_capacity(bank.entries.len());
            for entry in &bank.entries {
                let global_slot = sprite_request
                    .global_index(entry.local_index as u16)
                    .ok_or_else(|| SpriteExportError::MissingGlobalMapping {
                        path: path.clone(),
                        local_slot: entry.local_index,
                    })?;
                let relative_dir = format!(
                    "sources/{source_key}/registered/entry_{:04}_global_{global_slot:04}",
                    entry.local_index
                );
                let variants = variant_exporter.export_variants(
                    &relative_dir,
                    entry.selectors,
                    &entry.variants,
                )?;
                totals.physical_variants += variants.len();
                totals.pngs += variants.len();
                totals.raw_pixel_files += variants.len();
                registered_entries.push(RegisteredEntryRecord {
                    local_slot: entry.local_index,
                    global_slot,
                    source_span: entry.span.into(),
                    selectors: entry.selectors.into(),
                    variants,
                });
            }

            let mut orphan_entries = Vec::with_capacity(bank.orphan_entries.len());
            for (orphan_index, entry) in bank.orphan_entries.iter().enumerate() {
                let relative_dir = format!("sources/{source_key}/orphans/orphan_{orphan_index:03}");
                let variants = variant_exporter.export_variants(
                    &relative_dir,
                    entry.selectors,
                    &entry.variants,
                )?;
                totals.physical_variants += variants.len();
                totals.pngs += variants.len();
                totals.raw_pixel_files += variants.len();
                orphan_entries.push(OrphanEntryRecord {
                    orphan_index,
                    status: "ignored_by_retail_loader_count",
                    source_span: entry.span.into(),
                    selectors: entry.selectors.into(),
                    variants,
                });
            }
            (registered_entries, orphan_entries)
        };

        totals.source_ovls += 1;
        totals.raw_region_files += 1;
        totals.registered_entries += registered_entries.len();
        totals.orphan_entries += orphan_entries.len();
        sources.push(SpriteSource {
            ovl: name.to_owned(),
            framed_region_span: region.span.into(),
            raw_framed_region: raw_region_relative,
            main_payload_span: bank.main_payload.into(),
            descriptor_table_span: bank.descriptor_table.into(),
            requested_entries: region.requested_count,
            unreferenced_atlas_record_indices: bank.unreferenced_record_indices,
            registered_entries,
            orphan_entries,
        });
    }

    totals.unique_palette_files = palettes.assets.len();
    let report = SpriteExportReport {
        sources: totals.source_ovls,
        raw_region_files: totals.raw_region_files,
        registered_entries: totals.registered_entries,
        orphan_entries: totals.orphan_entries,
        physical_variants: totals.physical_variants,
        pngs: totals.pngs,
        raw_pixel_files: totals.raw_pixel_files,
        unique_palette_files: totals.unique_palette_files,
    };
    let manifest = SpriteManifest {
        schema_version: 1,
        kind: "infestation_handler_3_sprites",
        selected_shade: options.shade.to_string(),
        atlas_stride_bytes: SPRITE_ATLAS_STRIDE_BYTES,
        palette_layout: PaletteLayout {
            encoding: "rgb555_le",
            shade_levels: SPRITE_SHADE_LEVELS,
            organization: "color_index_major_32_word_shade_ramps",
            base_colors: "one_rgb555_word_per_color_after_all_shade_ramps",
        },
        ovl_filters: selected_filter_names,
        totals,
        palettes: palettes.assets,
        sources,
    };
    write_manifest(staged.path(), &manifest)?;
    staged.commit()?;
    Ok(report)
}

fn select_overlay_paths(
    overlay_paths: Vec<PathBuf>,
    filters: &[String],
) -> Result<(Vec<PathBuf>, Vec<String>, bool), SpriteExportError> {
    if filters.is_empty() {
        return Ok((overlay_paths, Vec::new(), false));
    }

    let mut requested = BTreeSet::new();
    for filter in filters {
        OverlayKey::parse_filename(filter).map_err(|source| {
            SpriteExportError::InvalidOverlayFilter {
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
        return Err(SpriteExportError::UnknownOverlayFilters {
            filters: unmatched.into_iter().collect(),
        });
    }
    Ok((selected, selected_names, true))
}

struct PaletteStore {
    staged_root: PathBuf,
    by_content: HashMap<Box<[u8]>, String>,
    assets: Vec<PaletteAsset>,
}

impl PaletteStore {
    fn new(staged_root: &Path) -> Self {
        Self {
            staged_root: staged_root.to_owned(),
            by_content: HashMap::new(),
            assets: Vec::new(),
        }
    }

    fn intern(
        &mut self,
        source_path: &Path,
        source: &[u8],
        span: ByteSpan,
    ) -> Result<String, SpriteExportError> {
        let bytes = span
            .get(source)
            .ok_or_else(|| SpriteExportError::PaletteSpanOutOfBounds {
                path: source_path.to_owned(),
                span,
            })?;
        if let Some(file) = self.by_content.get(bytes) {
            return Ok(file.clone());
        }

        let file = format!("palettes/palette_{:05}.rgb555le", self.assets.len());
        write(&self.staged_root.join(Path::new(&file)), bytes)?;
        self.by_content
            .insert(bytes.to_vec().into_boxed_slice(), file.clone());
        self.assets.push(PaletteAsset {
            file: file.clone(),
            byte_len: bytes.len(),
        });
        Ok(file)
    }
}

struct VariantExporter<'a, 'p> {
    source_path: &'a Path,
    source: &'a [u8],
    staged_root: &'a Path,
    shade: SpriteExportShade,
    bank: &'a inf_formats::SpriteBank,
    palettes: &'p mut PaletteStore,
}

impl VariantExporter<'_, '_> {
    fn export_variants(
        &mut self,
        relative_dir: &str,
        selectors: SpriteModeSelectors,
        variants: &[SpriteVariant],
    ) -> Result<Vec<VariantRecord>, SpriteExportError> {
        create_dir_all(&self.staged_root.join(Path::new(relative_dir)))?;
        variants
            .iter()
            .map(|variant| self.export_variant(relative_dir, selectors, variant))
            .collect()
    }

    fn export_variant(
        &mut self,
        relative_dir: &str,
        selectors: SpriteModeSelectors,
        variant: &SpriteVariant,
    ) -> Result<VariantRecord, SpriteExportError> {
        let stem = format!(
            "variant_{:02}_descriptor_{:04}",
            variant.physical_index, variant.sprite_id
        );
        let png_file = format!("{relative_dir}/{stem}.png");
        let raw_extension = match variant.encoding {
            SpriteEncoding::Indexed { .. } => "indices.u8",
            SpriteEncoding::DirectRgb555 { .. } => "rgb555le",
        };
        let raw_pixels_file = format!("{relative_dir}/{stem}.{raw_extension}");
        let pixels = match variant.encoding {
            SpriteEncoding::Indexed { indices, .. } => indices,
            SpriteEncoding::DirectRgb555 { pixels } => pixels,
        };
        let raw_pixels =
            pixels
                .get(self.source)
                .ok_or_else(|| SpriteExportError::PixelSpanOutOfBounds {
                    path: self.source_path.to_owned(),
                    span: pixels,
                })?;
        write(
            &self.staged_root.join(Path::new(&raw_pixels_file)),
            raw_pixels,
        )?;

        let rgba = variant
            .decode_rgba(self.source, self.shade.decoder_shade())
            .map_err(|source| SpriteExportError::Sprite {
                path: self.source_path.to_owned(),
                source,
            })?;
        let png_path = self.staged_root.join(Path::new(&png_file));
        let png = encode_rgba_png(variant.width, variant.height, &rgba).map_err(|source| {
            SpriteExportError::Png {
                path: png_path.clone(),
                source,
            }
        })?;
        write(&png_path, &png)?;

        let encoding = match variant.encoding {
            SpriteEncoding::Indexed { palette, .. } => EncodingRecord::Indexed {
                raw_pixel_format: "u8_palette_index",
                color_count: variant.color_count,
                palette_offset_in_main_payload: variant.palette_offset,
                palette_file: self
                    .palettes
                    .intern(self.source_path, self.source, palette.span)?,
                palette_span: palette.span.into(),
                shade_ramps_span: palette.shade_ramps.into(),
                base_colors_span: palette.base_colors.into(),
            },
            SpriteEncoding::DirectRgb555 { .. } => EncodingRecord::DirectRgb555 {
                raw_pixel_format: "rgb555_le",
                ignored_palette_offset_in_main_payload: variant.palette_offset,
            },
        };
        let atlas_record = &self.bank.atlas_records[variant.atlas_record_index];
        Ok(VariantRecord {
            physical_index: variant.physical_index,
            descriptor_global_id: variant.sprite_id,
            descriptor_span: variant.span.into(),
            selected_by_modes: selected_modes(selectors, variant.physical_index),
            flags: variant.flags.into(),
            width: variant.width,
            height: variant.height,
            atlas: AtlasRecord {
                byte_offset: variant.atlas_offset_bytes,
                x_bytes: variant.atlas_origin.x_bytes,
                y: variant.atlas_origin.y,
                atlas_record_index: variant.atlas_record_index,
                record_span: atlas_record.span.into(),
                pixel_span: atlas_record.pixels.into(),
            },
            png_file,
            raw_pixels_file,
            encoding,
        })
    }
}

fn selected_modes(selectors: SpriteModeSelectors, physical_index: u8) -> Vec<SpriteRenderMode> {
    [SpriteRenderMode::Mode0, SpriteRenderMode::Mode1]
        .into_iter()
        .filter(|mode| selectors.variant_index(*mode) == Some(usize::from(physical_index)))
        .collect()
}

fn encode_rgba_png(
    width: u16,
    height: u16,
    pixels: &[Rgba8],
) -> Result<Vec<u8>, png::EncodingError> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for pixel in pixels {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, u32::from(width), u32::from(height));
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
    }
    Ok(output)
}

fn write_manifest(root: &Path, manifest: &SpriteManifest) -> Result<(), SpriteExportError> {
    let manifest_path = root.join("manifest.json");
    let file = fs::File::create(&manifest_path).map_err(|source| SpriteExportError::Write {
        path: manifest_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, manifest).map_err(|source| {
        SpriteExportError::Json {
            path: manifest_path.clone(),
            source,
        }
    })?;
    writer.flush().map_err(|source| SpriteExportError::Write {
        path: manifest_path,
        source,
    })?;
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, SpriteExportError> {
    fs::read(path).map_err(|source| SpriteExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), SpriteExportError> {
    fs::create_dir_all(path).map_err(|source| SpriteExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SpriteExportError> {
    fs::write(path, bytes).map_err(|source| SpriteExportError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_argument_accepts_only_proven_levels() {
        assert_eq!("base".parse(), Ok(SpriteExportShade::Base));
        assert_eq!("BASE".parse(), Ok(SpriteExportShade::Base));
        assert_eq!("0".parse(), Ok(SpriteExportShade::Level(0)));
        assert_eq!("31".parse(), Ok(SpriteExportShade::Level(31)));
        assert!("32".parse::<SpriteExportShade>().is_err());
        assert!("-1".parse::<SpriteExportShade>().is_err());
        assert!("full".parse::<SpriteExportShade>().is_err());
    }

    #[test]
    fn filtered_export_is_repeatable_and_writes_rgba_png_and_raw_data() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root = temp_output("sprite-export-zero");
        let _ = fs::remove_dir_all(&output_root);
        let options = SpriteExportOptions {
            shade: SpriteExportShade::Base,
            ovl_filters: vec!["0.OVL".to_owned(), "0.ovl".to_owned()],
        };

        let first = export(&data_root, &output_root, &options).unwrap();
        assert_eq!(
            first,
            SpriteExportReport {
                sources: 1,
                raw_region_files: 1,
                registered_entries: 8,
                orphan_entries: 0,
                physical_variants: 8,
                pngs: 8,
                raw_pixel_files: 8,
                unique_palette_files: 8,
            }
        );
        let manifest_path = output_root.join("sprites/manifest.json");
        let first_manifest = fs::read(&manifest_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        assert_eq!(manifest["selected_shade"], "base");
        assert_eq!(manifest["ovl_filters"], serde_json::json!(["0.ovl"]));
        assert_eq!(
            manifest["sources"][0]["registered_entries"][0]["local_slot"],
            0
        );
        assert_eq!(
            manifest["sources"][0]["registered_entries"][0]["global_slot"],
            0
        );
        assert_eq!(
            manifest["sources"][0]["registered_entries"][0]["selectors"]["mode0_raw"],
            0
        );
        assert_eq!(
            manifest["sources"][0]["registered_entries"][0]["variants"][0]["descriptor_span"]
                ["byte_len"],
            28
        );
        let region = &manifest["sources"][0];
        let region_start = region["framed_region_span"]["start"].as_u64().unwrap() as usize;
        let region_end = region["framed_region_span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(
            fs::read(
                output_root
                    .join("sprites")
                    .join(region["raw_framed_region"].as_str().unwrap())
            )
            .unwrap(),
            fs::read(data_root.join("Overlay/0.ovl")).unwrap()[region_start..region_end]
        );

        let png_path = output_root.join(
            "sprites/sources/0/registered/entry_0000_global_0000/variant_00_descriptor_0000.png",
        );
        assert_rgba_png(&png_path, 32, 32);
        let raw_path = output_root.join(
            "sprites/sources/0/registered/entry_0000_global_0000/variant_00_descriptor_0000.indices.u8",
        );
        assert_eq!(fs::read(raw_path).unwrap().len(), 32 * 32);
        assert_eq!(
            fs::read_dir(output_root.join("sprites/palettes"))
                .unwrap()
                .count(),
            8
        );

        let second = export(&data_root, &output_root, &options).unwrap();
        assert_eq!(second, first);
        assert_eq!(fs::read(&manifest_path).unwrap(), first_manifest);

        let shade_png = fs::read(&png_path).unwrap();
        let shaded = SpriteExportOptions {
            shade: SpriteExportShade::Level(17),
            ovl_filters: vec!["0.ovl".to_owned()],
        };
        export(&data_root, &output_root, &shaded).unwrap();
        assert_ne!(fs::read(&png_path).unwrap(), shade_png);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["selected_shade"], "17");
        let _ = fs::remove_dir_all(output_root);
    }

    #[test]
    fn filtered_export_includes_the_ignored_9d2_entry() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let output_root = temp_output("sprite-export-orphan");
        let _ = fs::remove_dir_all(&output_root);
        let report = export(
            &data_root,
            &output_root,
            &SpriteExportOptions {
                shade: SpriteExportShade::Base,
                ovl_filters: vec!["9d2.ovl".to_owned()],
            },
        )
        .unwrap();
        assert_eq!(report.sources, 1);
        assert_eq!(report.registered_entries, 92);
        assert_eq!(report.orphan_entries, 1);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output_root.join("sprites/manifest.json")).unwrap())
                .unwrap();
        let orphan = &manifest["sources"][0]["orphan_entries"][0];
        assert_eq!(orphan["status"], "ignored_by_retail_loader_count");
        assert_eq!(orphan["variants"][0]["descriptor_global_id"], 1445);
        assert_eq!(
            orphan["variants"][0]["encoding"]["palette_offset_in_main_payload"],
            0x0012_cd20u32
        );
        assert!(output_root
            .join("sprites/sources/9d2/orphans/orphan_000/variant_00_descriptor_1445.png")
            .is_file());
        let _ = fs::remove_dir_all(output_root);
    }

    fn assert_rgba_png(path: &Path, expected_width: u32, expected_height: u32) {
        let decoder = png::Decoder::new(fs::File::open(path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (expected_width, expected_height));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(
            info.buffer_size(),
            (expected_width * expected_height * 4) as usize
        );
    }

    fn temp_output(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("infestation-{label}-{}", std::process::id()))
    }
}
