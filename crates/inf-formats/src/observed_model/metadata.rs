use super::{
    ensure, invalid,
    transcript::{hex, ParsedTranscript, DUMPS},
    ByteSpan, ObservationError, ObservationProvenance, TtdPosition, HEALTHSMALL_RETAIL_SHA256,
    HEALTHSMALL_SOURCE_SHA256,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Deserialize)]
pub(super) struct BundleMetadata {
    schema_version: u32,
    preset: String,
    status: String,
    trace: String,
    trace_bytes: u64,
    index_bytes: u64,
    retail_exe: String,
    retail_sha256: String,
    start_position: String,
    pub max_hits: u32,
    timeout_seconds: u32,
    hit_limit_reached: bool,
    observation: MetadataObservation,
    subject: Subject,
    stop_reason: String,
    entry_sha256: String,
    entry_matches_source: bool,
    identity_status: String,
    observed_hits: u32,
    last_observed_position: String,
}
#[derive(Deserialize)]
struct MetadataObservation {
    complete: bool,
    scans: u32,
    matches: u32,
    stage: u32,
    events: Vec<MetadataEvent>,
}
#[derive(Deserialize)]
struct MetadataEvent {
    stage: String,
    position: String,
    thread: u32,
    ctx: u32,
    entry: u32,
    args: u32,
    details: BTreeMap<String, String>,
    #[serde(flatten)]
    values: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Subject {
    overlay_offset: usize,
    entry_bytes: usize,
    sha256: String,
    f5_arguments_offset: usize,
    op05_arguments_offset: usize,
    vertex_slots: usize,
    dumps: BTreeMap<String, usize>,
}
impl BundleMetadata {
    pub(super) fn validate(&self) -> Result<(), ObservationError> {
        ensure(
            self.schema_version == 1 && self.preset == "HealthSmallDraw",
            "metadata is not a supported HealthSmallDraw bundle",
        )?;
        ensure(
            self.status == "completed"
                && self.observation.complete
                && !self.hit_limit_reached
                && self.stop_reason == "correlated_draw_end",
            "metadata does not describe a completed draw",
        )?;
        ensure(
            self.entry_matches_source
                && self.identity_status == "full_source_match"
                && self
                    .entry_sha256
                    .eq_ignore_ascii_case(HEALTHSMALL_SOURCE_SHA256),
            "metadata source identity is incomplete or mismatched",
        )?;
        ensure(
            self.retail_sha256
                .eq_ignore_ascii_case(HEALTHSMALL_RETAIL_SHA256),
            "unsupported retail executable fingerprint",
        )?;
        ensure(
            (1..=512).contains(&self.max_hits) && (10..=600).contains(&self.timeout_seconds),
            "metadata query bounds are unsupported",
        )?;
        ensure(
            !self.trace.is_empty()
                && self.trace.len() <= 4096
                && !self.retail_exe.is_empty()
                && self.retail_exe.len() <= 4096
                && self.trace_bytes > 0
                && self.index_bytes > 0,
            "metadata trace/executable provenance is incomplete",
        )?;
        let start: TtdPosition = self.start_position.parse()?;
        ensure(
            start.sequence != 0 || start.steps != 0,
            "metadata start position is zero",
        )?;
        let subject = &self.subject;
        ensure(
            subject.overlay_offset == 0x20eb8
                && subject.entry_bytes == 4696
                && subject
                    .sha256
                    .eq_ignore_ascii_case(HEALTHSMALL_SOURCE_SHA256)
                && subject.f5_arguments_offset == 0x418
                && subject.op05_arguments_offset == 0x420
                && subject.vertex_slots == 170,
            "metadata subject layout differs from canonical healthsmall",
        )?;
        ensure(
            subject.dumps.len() == DUMPS.len()
                && DUMPS
                    .iter()
                    .all(|spec| subject.dumps.get(spec.name) == Some(&spec.length)),
            "metadata dump names or sizes differ from the observation contract",
        )
    }
    pub(super) fn validate_transcript(
        &self,
        parsed: &ParsedTranscript,
    ) -> Result<(), ObservationError> {
        ensure(
            self.observation.scans == parsed.scans
                && self.observed_hits == parsed.scans
                && self.observation.matches == 1
                && self.observation.stage == 8
                && self.observation.events.len() == 8,
            "metadata counts differ from transcript",
        )?;
        ensure(
            parsed.events[0].position > self.start_position.parse()?,
            "observed invocation does not follow the requested start",
        )?;
        ensure(
            parsed.events[7].position == self.last_observed_position.parse()?,
            "metadata final position differs from transcript",
        )?;
        for ((metadata, event), fields) in self
            .observation
            .events
            .iter()
            .zip(&parsed.events)
            .zip(&parsed.fields)
        {
            ensure(
                metadata.stage == event.stage.as_str()
                    && metadata.position.parse::<TtdPosition>()? == event.position
                    && metadata.thread == event.thread_id
                    && metadata.ctx == event.context_address
                    && metadata.entry == event.entry_address
                    && metadata.args == event.args_address
                    && metadata.details == *fields,
                format!(
                    "metadata {} event differs from transcript",
                    event.stage.as_str()
                ),
            )?;
            let keys: &[&str] = match event.stage {
                super::ObservedModelStage::Call => &[
                    "object",
                    "vtable",
                    "target",
                    "selector",
                    "f5_args",
                    "return_site",
                ],
                super::ObservedModelStage::Return => &["value"],
                super::ObservedModelStage::Write => &["register1"],
                _ => &[],
            };
            for key in keys {
                let expected = u64::from(hex(fields, key)?);
                ensure(
                    metadata
                        .values
                        .get(*key)
                        .and_then(serde_json::Value::as_u64)
                        == Some(expected),
                    format!("metadata numeric {key} disagrees with observed value"),
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn provenance(
        &self,
        bundle: &Path,
        scans: u32,
    ) -> Result<ObservationProvenance, ObservationError> {
        Ok(ObservationProvenance {
            bundle_path: bundle.to_owned(),
            trace_path: self.trace.clone(),
            trace_bytes: self.trace_bytes,
            index_bytes: self.index_bytes,
            retail_exe: self.retail_exe.clone(),
            retail_sha256: self.retail_sha256.clone(),
            source_sha256: HEALTHSMALL_SOURCE_SHA256.to_owned(),
            source_overlay: "1.ovl".to_owned(),
            source_local_index: 166,
            source_global_slot: 168,
            source_span: ByteSpan::new(0x20eb8, 0x22110),
            start_position: self
                .start_position
                .parse()
                .map_err(|_| invalid("invalid provenance start position"))?,
            scanned_invocations: scans,
        })
    }
}
