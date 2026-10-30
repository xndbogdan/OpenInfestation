use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use inf_formats::{
    encode_sound_wav, parse_overlay, parse_sound_bank, ByteSpan, Error as FormatError, OverlayKey,
    Preload, SoundEntryKind, SoundError, WavError, SOUND_BITS_PER_SAMPLE, SOUND_CHANNELS,
    SOUND_SAMPLE_RATE,
};
use serde::Serialize;
use thiserror::Error;

use crate::installation;
use crate::managed_output::{ManagedOutputError, StagedOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundExportReport {
    pub sources: usize,
    pub raw_region_files: usize,
    pub slots: usize,
    pub pcm_wavs: usize,
    pub aliases: usize,
    pub unknown: usize,
    pub pcm_bytes: u64,
}

#[derive(Debug, Error)]
pub enum SoundExportError {
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

    #[error("failed to decode sounds in {path}: {source}")]
    Sound { path: PathBuf, source: SoundError },

    #[error("failed to encode a sound from {path}: {source}")]
    Wav { path: PathBuf, source: WavError },

    #[error("{path} refers to level {level}, which has no PreLoad.dat leaf")]
    MissingLeaf { path: PathBuf, level: u8 },

    #[error("{path} requests Handler 9 but its framed OVL has no Handler-9 region")]
    MissingSoundRegion { path: PathBuf },

    #[error("{path} Handler-9 local slot {local_slot} has no global resource mapping")]
    MissingGlobalMapping { path: PathBuf, local_slot: u32 },

    #[error("Handler-9 PCM span in {path} is outside the source OVL")]
    PcmOutOfBounds { path: PathBuf },

    #[error("failed to serialize {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("source path cannot be represented in the manifest: {0}")]
    InvalidSourcePath(PathBuf),

    #[error(transparent)]
    ManagedOutput(#[from] ManagedOutputError),
}

#[derive(Serialize)]
struct SoundManifest {
    schema_version: u32,
    kind: &'static str,
    audio_format: AudioFormat,
    totals: SoundTotals,
    sources: Vec<SoundSource>,
    sounds: Vec<SoundRecord>,
}

#[derive(Serialize)]
struct AudioFormat {
    sample_rate_hz: u32,
    channels: u16,
    bits_per_sample: u16,
    encoding: &'static str,
    provenance: &'static str,
}

#[derive(Default, Serialize)]
struct SoundTotals {
    source_ovls: usize,
    raw_region_files: usize,
    source_slots: usize,
    pcm_wavs: usize,
    aliases: usize,
    unknown: usize,
    pcm_bytes: u64,
}

#[derive(Serialize)]
struct SoundSource {
    ovl: String,
    framed_region_span: ManifestSpan,
    raw_framed_region: String,
    slots: usize,
    pcm_wavs: usize,
    aliases: usize,
    unknown: usize,
}

#[derive(Serialize)]
struct SoundRecord {
    ovl: String,
    local_slot: u32,
    global_slot: u16,
    record_span: ManifestSpan,
    raw_type: u32,
    fields_04_to_10: [u32; 4],
    #[serde(flatten)]
    data: SoundRecordData,
}

#[derive(Serialize)]
#[serde(tag = "entry_type", rename_all = "snake_case")]
enum SoundRecordData {
    Pcm {
        file: String,
        pcm_bytes: u32,
        samples: u32,
    },
    Alias {
        target_global_slot: i16,
        raw_target: u32,
        parameter_08_raw_16_16: u32,
        parameter_08: f64,
        parameter_0c_raw_16_16: u32,
        parameter_0c: f64,
        parameter_10_raw_16_16: u32,
        parameter_10: f64,
    },
    Unknown,
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

pub fn export(data_root: &Path, output_root: &Path) -> Result<SoundExportReport, SoundExportError> {
    let preload_path = data_root.join("PreLoad.dat");
    let preload_bytes = read(&preload_path)?;
    let preload = Preload::parse(&preload_bytes).map_err(|source| SoundExportError::Format {
        path: preload_path,
        source,
    })?;
    let target = output_root.join("sounds");
    let staged = StagedOutput::new(&target)?;
    let source_root = staged.path().join("sources");
    create_dir_all(&source_root)?;

    let overlay_dir = data_root.join("Overlay");
    let overlay_paths =
        installation::overlay_paths(data_root).map_err(|source| SoundExportError::Read {
            path: overlay_dir,
            source,
        })?;

    let mut totals = SoundTotals::default();
    let mut sources = Vec::new();
    let mut sounds = Vec::new();
    for path in overlay_paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SoundExportError::InvalidSourcePath(path.clone()))?;
        let key = OverlayKey::parse_filename(name).map_err(|source| SoundExportError::Format {
            path: path.clone(),
            source,
        })?;
        let leaf = preload
            .leaf(key.level)
            .ok_or_else(|| SoundExportError::MissingLeaf {
                path: path.clone(),
                level: key.level,
            })?;
        let requests = leaf.requests(key.request_kind());
        let sound_request = requests.get(9).expect("PreLoad has exactly 16 handlers");
        if sound_request.count == 0 {
            continue;
        }

        let bytes = read(&path)?;
        let layout =
            parse_overlay(&bytes, requests).map_err(|source| SoundExportError::Format {
                path: path.clone(),
                source,
            })?;
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 9)
            .ok_or_else(|| SoundExportError::MissingSoundRegion { path: path.clone() })?;
        let bank = parse_sound_bank(&bytes, region).map_err(|source| SoundExportError::Sound {
            path: path.clone(),
            source,
        })?;
        let source_key = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| SoundExportError::InvalidSourcePath(path.clone()))?;
        let source_dir = source_root.join(source_key);
        create_dir_all(&source_dir)?;
        let raw_region_relative = format!("sources/{source_key}/handler_09_framed.bin");
        write(
            &staged.path().join(&raw_region_relative),
            region
                .span
                .get(&bytes)
                .expect("overlay parser validated the Handler-9 region span"),
        )?;
        let mut source_pcm = 0usize;
        let mut source_aliases = 0usize;
        let mut source_unknown = 0usize;

        let bank_slot_count = bank.entries.len();
        for entry in bank.entries {
            let global_slot = sound_request
                .global_index(entry.local_index as u16)
                .ok_or_else(|| SoundExportError::MissingGlobalMapping {
                    path: path.clone(),
                    local_slot: entry.local_index,
                })?;
            let record_span = entry.record;
            let raw_type = entry.raw_type;
            let fields_04_to_10 = entry.fields_04_to_10;
            let data = match entry.kind {
                SoundEntryKind::Pcm {
                    data,
                    byte_len,
                    sample_count,
                } => {
                    let pcm = data
                        .get(&bytes)
                        .ok_or_else(|| SoundExportError::PcmOutOfBounds { path: path.clone() })?;
                    let filename = format!(
                        "sources/{source_key}/sound_{:03}_global_{global_slot:03}.wav",
                        entry.local_index
                    );
                    let wav = encode_sound_wav(pcm).map_err(|source| SoundExportError::Wav {
                        path: path.clone(),
                        source,
                    })?;
                    write(&staged.path().join(Path::new(&filename)), &wav)?;
                    source_pcm += 1;
                    totals.pcm_wavs += 1;
                    totals.pcm_bytes += u64::from(byte_len);
                    SoundRecordData::Pcm {
                        file: filename,
                        pcm_bytes: byte_len,
                        samples: sample_count,
                    }
                }
                SoundEntryKind::Alias {
                    raw_target,
                    target_global_slot,
                    pitch_spread_raw_16_16,
                    pitch_multiplier_raw_16_16,
                    gain_multiplier_raw_16_16,
                } => {
                    source_aliases += 1;
                    totals.aliases += 1;
                    SoundRecordData::Alias {
                        target_global_slot,
                        raw_target,
                        parameter_08_raw_16_16: pitch_spread_raw_16_16,
                        parameter_08: fixed_16_16(pitch_spread_raw_16_16),
                        parameter_0c_raw_16_16: pitch_multiplier_raw_16_16,
                        parameter_0c: fixed_16_16(pitch_multiplier_raw_16_16),
                        parameter_10_raw_16_16: gain_multiplier_raw_16_16,
                        parameter_10: fixed_16_16(gain_multiplier_raw_16_16),
                    }
                }
                SoundEntryKind::Unknown { .. } => {
                    source_unknown += 1;
                    totals.unknown += 1;
                    SoundRecordData::Unknown
                }
            };
            sounds.push(SoundRecord {
                ovl: name.to_owned(),
                local_slot: entry.local_index,
                global_slot,
                record_span: record_span.into(),
                raw_type,
                fields_04_to_10,
                data,
            });
        }

        totals.source_ovls += 1;
        totals.raw_region_files += 1;
        totals.source_slots += bank_slot_count;
        sources.push(SoundSource {
            ovl: name.to_owned(),
            framed_region_span: region.span.into(),
            raw_framed_region: raw_region_relative,
            slots: bank_slot_count,
            pcm_wavs: source_pcm,
            aliases: source_aliases,
            unknown: source_unknown,
        });
    }

    let report = SoundExportReport {
        sources: totals.source_ovls,
        raw_region_files: totals.raw_region_files,
        slots: totals.source_slots,
        pcm_wavs: totals.pcm_wavs,
        aliases: totals.aliases,
        unknown: totals.unknown,
        pcm_bytes: totals.pcm_bytes,
    };
    let manifest = SoundManifest {
        schema_version: 1,
        kind: "infestation_handler_9_sounds",
        audio_format: AudioFormat {
            sample_rate_hz: SOUND_SAMPLE_RATE,
            channels: SOUND_CHANNELS,
            bits_per_sample: SOUND_BITS_PER_SAMPLE,
            encoding: "signed_pcm_le",
            provenance: "Infest.exe WAVEFORMATEX at VA 0x00545558 (file offset 0x00145558)",
        },
        totals,
        sources,
        sounds,
    };
    let manifest_path = staged.path().join("manifest.json");
    let file = fs::File::create(&manifest_path).map_err(|source| SoundExportError::Write {
        path: manifest_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &manifest).map_err(|source| {
        SoundExportError::Json {
            path: manifest_path.clone(),
            source,
        }
    })?;
    writer.flush().map_err(|source| SoundExportError::Write {
        path: manifest_path,
        source,
    })?;
    drop(writer);
    staged.commit()?;
    Ok(report)
}

fn fixed_16_16(raw: u32) -> f64 {
    f64::from(raw) / 65_536.0
}

fn read(path: &Path) -> Result<Vec<u8>, SoundExportError> {
    fs::read(path).map_err(|source| SoundExportError::Read {
        path: path.to_owned(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), SoundExportError> {
    fs::create_dir_all(path).map_err(|source| SoundExportError::Write {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SoundExportError> {
    fs::write(path, bytes).map_err(|source| SoundExportError::Write {
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
            std::env::temp_dir().join(format!("infestation-sound-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&output_root);

        let first = export(&data_root, &output_root).unwrap();
        assert_eq!(
            first,
            SoundExportReport {
                sources: 2,
                raw_region_files: 2,
                slots: 77,
                pcm_wavs: 58,
                aliases: 19,
                unknown: 0,
                pcm_bytes: 2_541_972,
            }
        );
        let first_manifest = fs::read(output_root.join("sounds/manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&first_manifest).unwrap();
        let source = &manifest["sources"][0];
        let start = source["framed_region_span"]["start"].as_u64().unwrap() as usize;
        let end = source["framed_region_span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(
            fs::read(
                output_root
                    .join("sounds")
                    .join(source["raw_framed_region"].as_str().unwrap())
            )
            .unwrap(),
            fs::read(data_root.join("Overlay/0.ovl")).unwrap()[start..end]
        );
        assert_eq!(
            fs::read_dir(output_root.join("sounds/sources/0"))
                .unwrap()
                .count(),
            25
        );
        assert_eq!(
            fs::read_dir(output_root.join("sounds/sources/1"))
                .unwrap()
                .count(),
            35
        );
        let wav = fs::read(output_root.join("sounds/sources/0/sound_000_global_000.wav")).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");

        let second = export(&data_root, &output_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            fs::read(output_root.join("sounds/manifest.json")).unwrap(),
            first_manifest
        );
        let _ = fs::remove_dir_all(output_root);
    }
}
