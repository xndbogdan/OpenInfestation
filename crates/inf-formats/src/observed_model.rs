//! Validated, source-specific healthsmall draw observations. This is not a
//! `RuntimeFrame` producer or a general model executor.

mod metadata;
mod transcript;

use crate::{
    native_basis::NativeBasis, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, HandlerRequest,
    ModelEntry, ModelSubBlock, OverlayKey, OverlayVariant, ResourceSegment,
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const HEALTHSMALL_GLOBAL_SLOT: u16 = 168;
pub const HEALTHSMALL_SOURCE_SHA256: &str =
    "FC7131397DE35138999DE9C317FDF31CABBF1B77738714C17B764EBF66454A92";
pub const HEALTHSMALL_RETAIL_SHA256: &str =
    "BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDCCC90BD575882921E4956";
const ENTRY_BYTES: usize = 4696;
const CONTEXT_BYTES: usize = 428;
const SLOT_BYTES: usize = 24;
const VERTEX_SLOTS: usize = 170;

#[derive(Debug, Error)]
pub enum ObservationError {
    #[error("cannot read observation file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("observation file {path} exceeds {limit} bytes")]
    TooLarge { path: PathBuf, limit: usize },
    #[error("invalid healthsmall observation: {0}")]
    Invalid(String),
    #[error("invalid observation metadata: {0}")]
    Metadata(#[from] serde_json::Error),
}

/// Ordered numerical TTD position; neither component is a frame number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TtdPosition {
    pub sequence: u64,
    pub steps: u64,
}
impl std::fmt::Display for TtdPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:X}:{:X}", self.sequence, self.steps)
    }
}
impl std::str::FromStr for TtdPosition {
    type Err = ObservationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (sequence, steps) = value
            .split_once(':')
            .ok_or_else(|| invalid("invalid TTD position"))?;
        let part = |s: &str| {
            ensure(
                !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid TTD position component",
            )?;
            u64::from_str_radix(s, 16).map_err(|_| invalid("invalid TTD position component"))
        };
        Ok(Self {
            sequence: part(sequence)?,
            steps: part(steps)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedModelStage {
    Entry,
    F5,
    Call,
    Return,
    Write,
    Op05,
    Post05,
    End,
}
impl ObservedModelStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::F5 => "f5",
            Self::Call => "call",
            Self::Return => "return",
            Self::Write => "write",
            Self::Op05 => "op05",
            Self::Post05 => "post05",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedModelEvent {
    pub stage: ObservedModelStage,
    pub position: TtdPosition,
    pub thread_id: u32,
    pub context_address: u32,
    pub entry_address: u32,
    pub args_address: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedModelTransform {
    pub origin: [i32; 3],
    pub basis: NativeBasis,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedModelCallback {
    pub object_address: u32,
    pub vtable_address: u32,
    pub target_address: u32,
    pub selector: u32,
    pub result: u16,
    pub register1: u16,
}
/// Exactly the fields initialized by the observed far producer at depth >=64.
/// World XYZ and bytes +16/+17 remain only in `raw`, without validity claims.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedProjectionSlot {
    pub slot: u16,
    pub packed_xy: u32,
    pub depth_word: u32,
    pub clip: u8,
    pub fog: u8,
    pub raw: [u8; SLOT_BYTES],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationProvenance {
    pub bundle_path: PathBuf,
    pub trace_path: String,
    pub trace_bytes: u64,
    pub index_bytes: u64,
    pub retail_exe: String,
    pub retail_sha256: String,
    pub source_sha256: String,
    pub source_overlay: String,
    pub source_local_index: u16,
    pub source_global_slot: u16,
    pub source_span: ByteSpan,
    pub start_position: TtdPosition,
    pub scanned_invocations: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthSmallSnapshots {
    pub model_entry_before: Vec<u8>,
    pub model_entry: Vec<u8>,
    pub f5_context: Vec<u8>,
    pub f5_slot0: [u8; SLOT_BYTES],
    pub callback_return_context: Vec<u8>,
    pub post05_context: Vec<u8>,
    pub post05_vertex_slots: Vec<u8>,
    pub post05_projection_globals: [u8; 144],
    pub metadata_json: Vec<u8>,
    pub transcript: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthSmallObservation {
    pub provenance: ObservationProvenance,
    pub events: Vec<ObservedModelEvent>,
    /// Spans are relative to `snapshots.model_entry`; identity is physical 1.ovl local166/global168.
    pub source_model: ModelEntry,
    pub parent_context: u32,
    pub vertex_address: u32,
    pub near_far: u8,
    pub callback: ObservedModelCallback,
    pub incoming: ObservedModelTransform,
    pub nested: ObservedModelTransform,
    pub slot0_world: [i32; 3],
    pub projection_slots: Vec<ObservedProjectionSlot>,
    pub snapshots: HealthSmallSnapshots,
}

/// Independently checks bounded metadata, ordered transcript events, raw dump
/// addresses/bytes, retained files, and the late canonical source fingerprint.
/// Trace/executable paths and sizes are retained observations, not re-opened or
/// authenticated by hashing the multi-gigabyte recording.
pub fn load_healthsmall_bundle(
    bundle: &Path,
    source_entry: &[u8],
) -> Result<HealthSmallObservation, ObservationError> {
    ensure(
        source_entry.len() == ENTRY_BYTES && sha256(source_entry) == HEALTHSMALL_SOURCE_SHA256,
        "supplied source entry is not canonical healthsmall",
    )?;
    let metadata_json = read_bounded(&bundle.join("invocation.json"), 256 * 1024)?;
    let metadata: metadata::BundleMetadata = serde_json::from_slice(&metadata_json)?;
    metadata.validate()?;
    let transcript_bytes = read_bounded(&bundle.join("transcript.txt"), 2 * 1024 * 1024)?;
    let transcript =
        String::from_utf8(transcript_bytes).map_err(|_| invalid("transcript is not UTF-8"))?;
    let parsed = transcript::parse(&transcript, metadata.max_hits)?;
    metadata.validate_transcript(&parsed)?;
    let mut retained = std::collections::BTreeMap::new();
    for spec in transcript::DUMPS {
        let bytes = read_bounded(&bundle.join(spec.name), spec.length)?;
        ensure(
            bytes.len() == spec.length,
            format!("{} has wrong file length", spec.name),
        )?;
        ensure(
            parsed.dumps.get(spec.name) == Some(&bytes),
            format!("{} differs from its positioned transcript dump", spec.name),
        )?;
        retained.insert(spec.name, bytes);
    }
    ensure(
        retained["model-entry.bin"] == source_entry,
        "late model entry differs from canonical source bytes",
    )?;
    let take = |name: &'static str, map: &mut std::collections::BTreeMap<&'static str, Vec<u8>>| {
        map.remove(name).expect("all fixed dump names validated")
    };
    let snapshots = HealthSmallSnapshots {
        model_entry_before: take("model-entry-before.bin", &mut retained),
        model_entry: take("model-entry.bin", &mut retained),
        f5_context: take("f5-context.bin", &mut retained),
        f5_slot0: take("f5-slot0.bin", &mut retained)
            .try_into()
            .expect("validated length"),
        callback_return_context: take("callback-return-context.bin", &mut retained),
        post05_context: take("post05-context.bin", &mut retained),
        post05_vertex_slots: take("post05-vertex-slots.bin", &mut retained),
        post05_projection_globals: take("post05-projection-globals.bin", &mut retained)
            .try_into()
            .expect("validated length"),
        metadata_json,
        transcript,
    };
    let entry = &parsed.events[0];
    for (name, context) in [
        ("f5", &snapshots.f5_context),
        ("callback return", &snapshots.callback_return_context),
        ("post05", &snapshots.post05_context),
    ] {
        ensure(
            word(context, 0) == parsed.vertex_address
                && word(context, 4)
                    == parsed
                        .vertex_address
                        .checked_add(4080)
                        .ok_or_else(|| invalid("vertex address overflows"))?,
            format!("{name} context allocation differs from observed vertex bank"),
        )?;
        ensure(
            word(context, 0x1a8) == entry.entry_address
                && word(context, 0x9c) == parsed.parent_context,
            format!("{name} context entry/parent differs from observed identity"),
        )?;
        ensure(
            context[0x60] == 1,
            format!("{name} context is not the supported far path"),
        )?;
    }
    ensure(
        word(&snapshots.f5_context, 0x44) == parsed.callback.object_address,
        "F5 context callback object differs from actual call",
    )?;
    ensure(
        u16::from_le_bytes(snapshots.post05_context[0x72..0x74].try_into().unwrap())
            == parsed.callback.register1,
        "post05 context register1 differs from observed write",
    )?;
    let incoming = transform(&snapshots.f5_context)?;
    let nested = transform(&snapshots.post05_context)?;
    let slot0_world = xyz(&snapshots.f5_slot0, 0);
    ensure(
        slot0_world == incoming.origin && slot0_world == nested.origin,
        "slot0 world origin disagrees with observed transforms",
    )?;
    ensure(
        snapshots.post05_vertex_slots[..12] == snapshots.f5_slot0[..12],
        "post05 slot0 world origin changed without a producer",
    )?;
    ensure(
        snapshots.post05_projection_globals[..52] == snapshots.post05_context[8..60],
        "published world origin/basis differs from nested context",
    )?;
    let mut projection_slots = Vec::with_capacity((VERTEX_SLOTS - 2) / 2);
    for slot in (2..VERTEX_SLOTS).step_by(2) {
        let raw: [u8; 24] = snapshots.post05_vertex_slots[slot * 24..(slot + 1) * 24]
            .try_into()
            .unwrap();
        let depth_word = word(&raw, 16);
        ensure(
            (depth_word as i32) >= 64,
            format!("slot {slot} far depth does not establish fresh projected XY/fog"),
        )?;
        projection_slots.push(ObservedProjectionSlot {
            slot: slot as u16,
            packed_xy: word(&raw, 12),
            depth_word,
            clip: raw[20],
            fog: raw[21],
            raw,
        });
    }
    Ok(HealthSmallObservation {
        provenance: metadata.provenance(bundle, parsed.scans)?,
        events: parsed.events,
        source_model: parse_source_model(source_entry)?,
        parent_context: parsed.parent_context,
        vertex_address: parsed.vertex_address,
        near_far: 1,
        callback: parsed.callback,
        incoming,
        nested,
        slot0_world,
        projection_slots,
        snapshots,
    })
}

fn parse_source_model(bytes: &[u8]) -> Result<ModelEntry, ObservationError> {
    // Frame the existing entry slice without inserting or modifying any bytes.
    let span = ByteSpan::new(0, bytes.len());
    let region = HandlerRegion {
        handler: HandlerId::MODELS,
        requested_count: 1,
        span,
        framing: HandlerFraming::ModelSubBlocks {
            total_entries: 1,
            blocks: vec![ModelSubBlock {
                span,
                payload: span,
                entry_count: 1,
            }],
        },
    };
    let request = HandlerRequest {
        count: 1,
        segments: vec![ResourceSegment {
            local_start: 0,
            global_delta: 168,
        }],
    };
    let mut bank = crate::parse_model_bank(
        bytes,
        &region,
        &request,
        &OverlayKey {
            level: 1,
            variant: OverlayVariant::Base,
        },
    )
    .map_err(|error| invalid(format!("canonical model parse: {error}")))?;
    let mut model = bank.entries.remove(0);
    model.identity.local_index = 166;
    Ok(model)
}
fn transform(bytes: &[u8]) -> Result<ObservedModelTransform, ObservationError> {
    let basis = NativeBasis {
        words: std::array::from_fn(|index| word(bytes, 0x14 + index * 4)),
    };
    ensure(
        basis.words[1..]
            .iter()
            .all(|value| f32::from_bits(*value).is_finite()),
        "observed basis has non-finite components",
    )?;
    Ok(ObservedModelTransform {
        origin: xyz(bytes, 8),
        basis,
    })
}
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed snapshot layout"),
    )
}
fn xyz(bytes: &[u8], offset: usize) -> [i32; 3] {
    std::array::from_fn(|index| word(bytes, offset + index * 4) as i32)
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn invalid(message: impl Into<String>) -> ObservationError {
    ObservationError::Invalid(message.into())
}
fn ensure(condition: bool, message: impl Into<String>) -> Result<(), ObservationError> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, ObservationError> {
    let file = File::open(path).map_err(|source| ObservationError::Io {
        path: path.into(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| ObservationError::Io {
            path: path.into(),
            source,
        })?;
    if bytes.len() > limit {
        return Err(ObservationError::TooLarge {
            path: path.into(),
            limit,
        });
    }
    Ok(bytes)
}
