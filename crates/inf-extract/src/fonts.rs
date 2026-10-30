//! Handler-4 font metadata and lossless source regions. Decoding is owned by
//! inf-formats; font selection, layout, and glyph drawing remain runtime work.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    parse_font_bank, parse_overlay, ByteSpan, Error as FormatError, Font, FontError,
    HandlerRequest, OverlayKey, Preload, RequestKind,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FontExportReport {
    pub sources: usize,
    pub fonts: usize,
    pub glyph_metrics: usize,
    pub sprite_arguments: usize,
    pub payload_bytes: usize,
    pub framed_bytes: usize,
}

#[derive(Debug, Error)]
pub enum FontExportError {
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
    #[error("failed to decode fonts in {path}: {source}")]
    Fonts { path: PathBuf, source: FontError },
    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },
    #[error("{path} requests Handler 4 but its framed OVL has no Handler-4 region")]
    MissingFontRegion { path: PathBuf },
    #[error("{path} Handler-4 local slot {local_slot} has no global resource mapping")]
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
struct FontManifest {
    schema_version: u32,
    kind: &'static str,
    provenance: FontProvenance,
    totals: FontExportReport,
    sources: Vec<FontSource>,
}

#[derive(Serialize)]
struct FontProvenance {
    decoder: &'static str,
    raw_bytes: &'static str,
    spans: &'static str,
    global_mapping: &'static str,
    header_words: &'static str,
    sprite_arguments: &'static str,
    layout: &'static str,
}

#[derive(Serialize)]
struct FontSource {
    ovl: String,
    ovl_bytes: usize,
    key: OverlayKey,
    request_kind: RequestKind,
    request: HandlerRequest,
    raw_framed_region: String,
    framed_region: ByteSpan,
    payload: ByteSpan,
    header_table: ByteSpan,
    trailing: ByteSpan,
    fonts: Vec<FontRecord>,
}

#[derive(Serialize)]
struct FontRecord {
    global_slot: u16,
    #[serde(flatten)]
    font: Font,
}

pub fn export(data_root: &Path, output_root: &Path) -> Result<FontExportReport, FontExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload =
        Preload::parse(&read(&preload_path)?).map_err(|source| FontExportError::Format {
            path: preload_path,
            source,
        })?;
    let staged = StagedOutput::new(&output_root.join("fonts"))?;
    let source_root = staged.path().join("sources");
    fs::create_dir_all(&source_root).map_err(|source| FontExportError::Write {
        path: source_root,
        source,
    })?;
    let paths = installation::overlay_paths(data_root).map_err(|source| FontExportError::Read {
        path: data_root.join("Overlay"),
        source,
    })?;
    let mut totals = FontExportReport::default();
    let mut sources = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| FontExportError::InvalidSourcePath(path.clone()))?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| FontExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| FontExportError::Format {
            path: path.clone(),
            source,
        })?;
        let request_kind = key.request_kind();
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| FontExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(request_kind);
        let request = requests.get(4).expect("PreLoad has exactly 16 handlers");
        if request.count == 0 {
            continue;
        }
        let bytes = read(&path)?;
        let layout = parse_overlay(&bytes, requests).map_err(|source| FontExportError::Format {
            path: path.clone(),
            source,
        })?;
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 4)
            .ok_or_else(|| FontExportError::MissingFontRegion { path: path.clone() })?;
        let bank = parse_font_bank(&bytes, region).map_err(|source| FontExportError::Fonts {
            path: path.clone(),
            source,
        })?;
        let raw_framed_region = format!("sources/{stem}.bin");
        let raw_path = staged.path().join(&raw_framed_region);
        fs::write(
            &raw_path,
            region.span.get(&bytes).expect("validated region"),
        )
        .map_err(|source| FontExportError::Write {
            path: raw_path,
            source,
        })?;
        let mut fonts = Vec::with_capacity(bank.fonts.len());
        for font in bank.fonts {
            let global_slot = request
                .global_index(font.local_index as u16)
                .ok_or_else(|| FontExportError::MissingGlobalMapping {
                    path: path.clone(),
                    local_slot: font.local_index,
                })?;
            totals.glyph_metrics += font.metrics.len();
            totals.sprite_arguments += font.sprite_words.len();
            fonts.push(FontRecord { global_slot, font });
        }
        totals.sources += 1;
        totals.fonts += fonts.len();
        totals.payload_bytes += bank.payload.len();
        totals.framed_bytes += region.span.len();
        sources.push(FontSource {
            ovl: name.to_owned(),
            ovl_bytes: bytes.len(),
            key,
            request_kind,
            request: request.clone(),
            raw_framed_region,
            framed_region: region.span,
            payload: bank.payload,
            header_table: bank.header_table,
            trailing: bank.trailing,
            fonts,
        });
    }
    let manifest = FontManifest {
        schema_version: 1,
        kind: "infestation_handler_4_fonts",
        provenance: FontProvenance {
            decoder: "inf-formats::parse_font_bank; loader 0050F250; text consumers 004A3B20/004A3E30/004A4240.",
            raw_bytes: "Each .bin retains the complete Handler-4 region including its size prefix. No pointer fixups or source-byte changes are applied.",
            spans: "Every span is absolute within the source OVL. Subtract framed_region.start to address the exported raw file.",
            global_mapping: "FUN_004EFC00 selects the final request segment whose local_start does not exceed local_index, then adds its signed global_delta to obtain the Handler-4 global_slot.",
            header_words: "All sixteen disk words are retained, including unknown fields and the +38/+3C pointer placeholders overwritten by the runtime loader.",
            sprite_arguments: "sprite_words retain full DWORD callback arguments in source order. Their signed low WORD indexes the global Handler-3 pool; these are not Handler-4 local slots.",
            layout: "Metrics retain signed native font units. Direct mode has one sprite argument per glyph; PositionGrid has glyph_count * columns * rows. This export does not choose font scale, glyph rendering variants, or text layout.",
        },
        totals,
        sources,
    };
    write_json(&staged.path().join("manifest.json"), &manifest)?;
    staged.commit()?;
    Ok(totals)
}

fn read(path: &Path) -> Result<Vec<u8>, FontExportError> {
    fs::read(path).map_err(|source| FontExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), FontExportError> {
    let file = fs::File::create(path).map_err(|source| FontExportError::Write {
        path: path.to_owned(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|source| FontExportError::Json {
        path: path.to_owned(),
        source,
    })?;
    writer.flush().map_err(|source| FontExportError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod tests;
