//! Legacy staged JSONL schema for retail Handler-8 runtime frames.
//!
//! The passive inspector (`inf-inspect`) reads only stable registration tables
//! and handle lists. It cannot observe the transient render context built at
//! `FUN_0049FEA0`/`FUN_004A00E0`. This speculative v1 layout does not carry
//! positioned callback results or per-field vertex validity. Its 64-WORD
//! register window overlaps the parent pointer and saved basis; only the
//! first 22 WORDs are independent register storage.
//!
//! There is no producer or viewer reader for this layout. The accepted
//! healthsmall draw bundles use [`crate::observed_model`] instead, with actual
//! callback returns, native bases and explicit slot custody. Do not fabricate
//! these legacy records from passive samples or new draw observations.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scene::SceneContext;

/// Schema version emitted by [`ReplayCaptureMeta::new`] and required by
/// [`ReplayCaptureMeta::validate`] and [`RuntimeFrame::validate`].
pub const REPLAY_SCHEMA_VERSION: u32 = 1;

/// Record discriminator for [`ReplayCaptureMeta`] lines.
pub const REPLAY_CAPTURE_META_KIND: &str = "capture_meta";
/// Record discriminator for [`RuntimeFrame`] lines.
pub const REPLAY_FRAME_KIND: &str = "runtime_frame";
/// Legacy raw window length at `+0x70`, not an independent register count.
pub const ANIMATION_REGISTER_COUNT: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("expected record kind {expected}, found {found:?}")]
    WrongRecordKind {
        expected: &'static str,
        found: String,
    },
    #[error("unsupported replay schema version {found}, this build reads {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("expected {expected} animation registers, found {found}")]
    WrongRegisterCount { expected: usize, found: usize },
}

/// File header for one captured-frame JSONL stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayCaptureMeta {
    pub record_kind: String,
    pub schema_version: u32,
    pub tool: String,
    pub scene: SceneContext,
    pub evidence_scope: String,
}

impl ReplayCaptureMeta {
    pub fn new(tool: impl Into<String>, scene: SceneContext) -> Self {
        Self {
            record_kind: REPLAY_CAPTURE_META_KIND.to_owned(),
            schema_version: REPLAY_SCHEMA_VERSION,
            tool: tool.into(),
            scene,
            evidence_scope: "observational breakpoint trace at FUN_0049FEA0/FUN_004A00E0; callback addresses and operands recorded, never invoked".to_owned(),
        }
    }

    pub fn validate(&self) -> Result<(), ReplayError> {
        if self.record_kind != REPLAY_CAPTURE_META_KIND {
            return Err(ReplayError::WrongRecordKind {
                expected: REPLAY_CAPTURE_META_KIND,
                found: self.record_kind.clone(),
            });
        }
        if self.schema_version != REPLAY_SCHEMA_VERSION {
            return Err(ReplayError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: REPLAY_SCHEMA_VERSION,
            });
        }
        Ok(())
    }
}

/// One recorded callback observation: the callback address plus the packed
/// operand words observed at the call site. The callback itself is never
/// invoked by any tool reading this record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackObservation {
    pub address: u32,
    pub operands: Vec<u16>,
}

/// Legacy untyped child-basis window. Accepted draw observations instead use
/// the proven counter-plus-nine-f32 [`crate::native_basis::NativeBasis`] shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildBasisObservation {
    pub child_slot: u16,
    pub raw_words: Vec<u16>,
}

/// One captured retail draw: the root Handler-8 global slot plus the raw
/// transient context the passive inspector cannot see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFrame {
    pub record_kind: String,
    pub frame: u64,
    pub root_global_slot: u16,
    pub animation_registers: Vec<u16>,
    pub callbacks: Vec<CallbackObservation>,
    pub child_bases: Vec<ChildBasisObservation>,
    /// Raw mode word; semantics unlabeled.
    pub mode_raw: u16,
    /// Raw difficulty word; semantics unlabeled.
    pub difficulty_raw: u16,
}

impl RuntimeFrame {
    pub fn validate(&self) -> Result<(), ReplayError> {
        if self.record_kind != REPLAY_FRAME_KIND {
            return Err(ReplayError::WrongRecordKind {
                expected: REPLAY_FRAME_KIND,
                found: self.record_kind.clone(),
            });
        }
        if self.animation_registers.len() != ANIMATION_REGISTER_COUNT {
            return Err(ReplayError::WrongRegisterCount {
                expected: ANIMATION_REGISTER_COUNT,
                found: self.animation_registers.len(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame() -> RuntimeFrame {
        RuntimeFrame {
            record_kind: REPLAY_FRAME_KIND.to_owned(),
            frame: 7,
            root_global_slot: 221,
            animation_registers: vec![0; ANIMATION_REGISTER_COUNT],
            callbacks: vec![CallbackObservation {
                address: 0x0040_0000,
                operands: vec![1, 2, 3],
            }],
            child_bases: vec![ChildBasisObservation {
                child_slot: 43,
                raw_words: vec![4, 5],
            }],
            mode_raw: 0,
            difficulty_raw: 4,
        }
    }

    #[test]
    fn capture_meta_accepts_its_own_schema_version() {
        let meta = ReplayCaptureMeta::new(
            "observational-breakpoint-trace",
            SceneContext::new(17, 4, 0),
        );
        assert_eq!(meta.schema_version, REPLAY_SCHEMA_VERSION);
        meta.validate().unwrap();
        let round_tripped: ReplayCaptureMeta =
            serde_json::from_str(&serde_json::to_string(&meta).unwrap()).unwrap();
        assert_eq!(round_tripped, meta);
    }

    #[test]
    fn frames_round_trip_and_require_sixty_four_registers() {
        let frame = sample_frame();
        frame.validate().unwrap();
        let round_tripped: RuntimeFrame =
            serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        assert_eq!(round_tripped, frame);

        let short = RuntimeFrame {
            animation_registers: vec![0; 63],
            ..frame
        };
        assert_eq!(
            short.validate(),
            Err(ReplayError::WrongRegisterCount {
                expected: 64,
                found: 63
            })
        );
    }

    #[test]
    fn foreign_kinds_and_versions_are_rejected() {
        let mut meta = ReplayCaptureMeta::new("tool", SceneContext::new(1, 0, 0));
        meta.schema_version += 1;
        assert_eq!(
            meta.validate(),
            Err(ReplayError::UnsupportedSchemaVersion {
                found: REPLAY_SCHEMA_VERSION + 1,
                supported: REPLAY_SCHEMA_VERSION
            })
        );
        let mut frame = sample_frame();
        frame.record_kind = "model_table_sample".to_owned();
        assert_eq!(
            frame.validate(),
            Err(ReplayError::WrongRecordKind {
                expected: REPLAY_FRAME_KIND,
                found: "model_table_sample".to_owned()
            })
        );
    }
}
