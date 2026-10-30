//! Asset readability audit, separate from lossless container coverage.
//! This command never regenerates assets or labels opaque bytes as decoded.

use std::{collections::BTreeMap, error::Error, fs, path::Path};

use inf_formats::{
    analyze_model_diagnostic, decode_sample_grid, parse_constructor_asset_bank, parse_font_bank,
    parse_handler10_bank, parse_handler5_bank, parse_material_bank, parse_mission_parameters,
    parse_mission_placements, parse_model_bank, parse_sound_bank, parse_sprite_bank,
    parse_string_bank, parse_terrain_bank, parse_word_resource_bank, parse_world_instance_bank,
    ByteSpan, ConstructorPayloadView, HandlerRegion, ModelDiagnosticCompleteness,
    ModelDiagnosticOmission, ModelDiagnosticProjection, ModelDiagnosticUnavailableKind,
    ModelDynamicDependency, ModelEntry, OverlayKey, Preload, RequestTable, SampleGridRole,
    SoundEntryKind,
};
use serde::Serialize;

use crate::inventory::{self, InventorySummary};

#[derive(Debug, Serialize)]
pub struct CoverageReport {
    pub summary: CoverageSummary,
    pub model_variants: Vec<ModelVariant>,
}

#[derive(Debug, Serialize)]
pub struct CoverageSummary {
    pub schema_version: u32,
    pub scope: &'static str,
    pub interpretation: &'static str,
    pub inventory: InventorySummary,
    pub handlers: Vec<HandlerCoverage>,
    pub models: ModelCoverage,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    TypedAssetData,
    RawWords,
    PartialRecordSemantics,
    RuntimeDependentPrograms,
}

#[derive(Debug, Serialize)]
pub struct HandlerCoverage {
    pub handler: u8,
    pub family: &'static str,
    pub knowledge: Knowledge,
    pub limitations: &'static str,
    pub regions: usize,
    pub framed_bytes: usize,
    pub requested_entries: usize,
    /// Entries accepted by a canonical payload parser, not semantic completion.
    pub parsed_entries: usize,
    pub details: BTreeMap<&'static str, usize>,
}

#[derive(Debug, Default, Serialize)]
pub struct ModelCoverage {
    pub physical_entries: usize,
    pub global_slots: usize,
    pub unique_slot_content_variants: usize,
    pub physical_diagnostics: BTreeMap<&'static str, usize>,
    pub unique_variant_diagnostics: BTreeMap<&'static str, usize>,
    pub physical_auxiliary: ModelAuxiliaryCoverage,
    pub unique_variant_auxiliary: ModelAuxiliaryCoverage,
}

#[derive(Debug, Default, Serialize)]
pub struct ModelAuxiliaryCoverage {
    pub active_programs: usize,
    /// Header raw_0e is zero, including entries with no auxiliary pool.
    pub inactive_entries: usize,
    /// Inactive bytes remain lossless source data, not decoded instructions.
    pub inactive_entries_with_auxiliary_bytes: usize,
    /// Counts exclude the required 0x88 terminator and trailing padding.
    pub instructions: usize,
    pub opcode_instructions: BTreeMap<u8, usize>,
}

impl ModelAuxiliaryCoverage {
    fn insert(&mut self, entry: &ModelEntry) {
        if let Some(program) = &entry.auxiliary_program {
            self.active_programs += 1;
            self.instructions += program.instructions.len();
            for instruction in &program.instructions {
                count(&mut self.opcode_instructions, instruction.opcode, 1);
            }
        } else {
            self.inactive_entries += 1;
            self.inactive_entries_with_auxiliary_bytes +=
                usize::from(!entry.auxiliary_span.is_empty());
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModelOccurrence {
    pub ovl: String,
    pub local_index: u16,
    pub content_span: ByteSpan,
}

#[derive(Debug, Serialize)]
pub struct ModelVariant {
    pub global_slot: u16,
    pub name: Option<String>,
    pub diagnostic: &'static str,
    pub reason: Option<String>,
    pub omissions: Vec<ModelDiagnosticOmission>,
    pub dependencies: Vec<ModelDynamicDependency>,
    pub occurrences: Vec<ModelOccurrence>,
}

fn handler_coverage(handler: u8) -> HandlerCoverage {
    use Knowledge::*;
    let (family, knowledge, limitations) = match handler {
        0 => ("strings", TypedAssetData, "Original byte strings; encoding and label callback behavior remain consumer concerns."),
        1 | 2 | 6 => ("word_resources", RawWords, "Four-byte words and registration are known; consumer roles are not fully named."),
        3 => ("sprites", TypedAssetData, "Selectors, pixels, palettes and flags decode; GPU upload, animation use and final composition are separate."),
        4 => ("fonts", TypedAssetData, "Glyph metrics, sprite references and modes decode; unknown header words are preserved."),
        5 => ("display_records", PartialRecordSemantics, "Record boundaries are known; packed display-policy fields require RE."),
        10 => ("sample_grids", PartialRecordSemantics, "Known global roles expose signed sample grids and source metadata; full terrain/map pixel composition remains separate."),
        7 => ("materials", PartialRecordSemantics, "Full material words decode; display conversion and unresolved terrain roles are consumer-specific."),
        8 => ("models", RuntimeDependentPrograms, "Render programs and enabled auxiliary programs decode structurally; raw_0e zero leaves auxiliary bytes inactive and raw. Diagnostic availability does not establish complete assembly, animation, collision outcomes or runtime execution."),
        9 => ("sounds", TypedAssetData, "PCM and alias records decode; alias playback/lifetime and any unknown kinds remain explicit."),
        11 | 15 => ("mission_records", PartialRecordSemantics, "Fixed records preserve raw fields; names do not prove entity or mission meaning."),
        12 => ("terrain", TypedAssetData, "Grid channels and sparse records decode; complete world placement, lighting and composition remain separate."),
        13 => ("constructor_resources", PartialRecordSemantics, "Dispatch, initial model bindings, proven shared fields and selector70's relocated point table decode; other derived-class extensions and execution remain open."),
        14 => ("world_placements", PartialRecordSemantics, "World placements and ordered instance command graphs decode; command-specific views require resolved class context, while derived payloads, header meanings and final coordinates remain open."),
        _ => unreachable!("the PreLoad dispatch has sixteen handlers"),
    };
    HandlerCoverage {
        handler,
        family,
        knowledge,
        limitations,
        regions: 0,
        framed_bytes: 0,
        requested_entries: 0,
        parsed_entries: 0,
        details: BTreeMap::new(),
    }
}

fn count<K: Ord>(counts: &mut BTreeMap<K, usize>, key: K, value: usize) {
    *counts.entry(key).or_default() += value;
}

struct ModelAudit {
    /// Compare exact content, not hashes, global IDs alone, or padding bytes.
    variants: BTreeMap<(u16, Vec<u8>), ModelVariant>,
    summary: ModelCoverage,
}

impl ModelAudit {
    fn insert(&mut self, source: &[u8], ovl: &str, entry: ModelEntry) {
        let key = (
            entry.identity.global_index,
            entry
                .content_span
                .get(source)
                .expect("parsed content span")
                .to_vec(),
        );
        let occurrence = ModelOccurrence {
            ovl: ovl.to_owned(),
            local_index: entry.identity.local_index,
            content_span: entry.content_span,
        };
        self.summary.physical_entries += 1;
        self.summary.physical_auxiliary.insert(&entry);
        if let Some(existing) = self.variants.get_mut(&key) {
            count(
                &mut self.summary.physical_diagnostics,
                existing.diagnostic,
                1,
            );
            existing.occurrences.push(occurrence);
            return;
        }
        self.summary.unique_variant_auxiliary.insert(&entry);
        let (diagnostic, reason, omissions, dependencies) = match analyze_model_diagnostic(
            inf_formats::ModelDiagnosticRequest::unconditioned(&entry),
        ) {
            ModelDiagnosticProjection::Ready(mesh) => (
                match mesh.completeness {
                    ModelDiagnosticCompleteness::CompleteMesh => "complete_root_mesh",
                    ModelDiagnosticCompleteness::PartialMesh => "partial_mesh",
                },
                None,
                mesh.omitted_effects,
                mesh.unresolved_dependencies,
            ),
            ModelDiagnosticProjection::Unavailable(gap) => (
                match gap.kind {
                    ModelDiagnosticUnavailableKind::Empty => "empty",
                    ModelDiagnosticUnavailableKind::NoPolygonGeometry => "no_polygon_geometry",
                    ModelDiagnosticUnavailableKind::RuntimeDependentGeometry => {
                        "runtime_dependent_geometry"
                    }
                    ModelDiagnosticUnavailableKind::NestedProgram => "nested_program",
                    ModelDiagnosticUnavailableKind::ModelInstance => "model_instance",
                    ModelDiagnosticUnavailableKind::UnresolvedCommandEffects => {
                        "unresolved_command_effects"
                    }
                },
                Some(gap.reason),
                gap.omitted_effects,
                gap.unresolved_dependencies,
            ),
        };
        count(&mut self.summary.physical_diagnostics, diagnostic, 1);
        count(&mut self.summary.unique_variant_diagnostics, diagnostic, 1);
        self.variants.insert(
            key,
            ModelVariant {
                global_slot: entry.identity.global_index,
                name: entry
                    .name
                    .as_ref()
                    .and_then(|n| n.ascii(source))
                    .map(str::to_owned),
                diagnostic,
                reason,
                omissions,
                dependencies,
                occurrences: vec![occurrence],
            },
        );
    }
}

/// Validate each canonical payload parser while counting its current scope.
/// A parse failure aborts with file/handler provenance, never a coverage success.
fn audit_region(
    source: &[u8],
    region: &HandlerRegion,
    requests: &RequestTable,
    key: &OverlayKey,
    ovl: &str,
    coverage: &mut HandlerCoverage,
    models: &mut ModelAudit,
) -> Result<usize, Box<dyn Error>> {
    let details = &mut coverage.details;
    Ok(match region.handler.get() {
        0 => parse_string_bank(source, region)?.entries.len(),
        1 | 2 | 6 => parse_word_resource_bank(source, region)?.entries.len(),
        3 => {
            let bank = parse_sprite_bank(source, region)?;
            count(
                details,
                "registered_variants",
                bank.entries.iter().map(|e| e.variants.len()).sum(),
            );
            count(details, "loader_ignored_entries", bank.orphan_entries.len());
            bank.entries.len()
        }
        4 => {
            let bank = parse_font_bank(source, region)?;
            count(
                details,
                "glyph_metrics",
                bank.fonts.iter().map(|f| f.metrics.len()).sum(),
            );
            count(
                details,
                "sprite_arguments",
                bank.fonts.iter().map(|f| f.sprite_words.len()).sum(),
            );
            bank.fonts.len()
        }
        5 => parse_handler5_bank(source, region)?.records.len(),
        7 => parse_material_bank(source, region)?.entries.len(),
        8 => {
            let bank = parse_model_bank(source, region, &requests.0[8], key)?;
            let entries = bank.entries.len();
            for entry in bank.entries {
                models.insert(source, ovl, entry);
            }
            entries
        }
        9 => {
            let bank = parse_sound_bank(source, region)?;
            for entry in &bank.entries {
                count(
                    details,
                    match entry.kind {
                        SoundEntryKind::Pcm { .. } => "pcm",
                        SoundEntryKind::Alias { .. } => "aliases",
                        SoundEntryKind::Unknown { .. } => "unknown_kinds",
                    },
                    1,
                );
            }
            bank.entries.len()
        }
        10 => {
            let bank = parse_handler10_bank(source, region)?;
            let mut decoded_spans = Vec::new();
            for record in &bank.records {
                let global = requests.0[10]
                    .global_index(u16::try_from(record.local_index)?)
                    .ok_or("Handler-10 local slot has no global mapping")?;
                if let Some(role) = SampleGridRole::from_global_slot(global) {
                    let grid = decode_sample_grid(source, bank.payload, record, role)?;
                    count(details, "sample_grids", 1);
                    count(details, "sample_bytes", grid.samples_span.len());
                    count(
                        details,
                        match role {
                            SampleGridRole::EightWordSamples => "eight_word_samples",
                            SampleGridRole::FourSampleMask => "mask_samples",
                            SampleGridRole::MaterialIndices => "material_index_samples",
                        },
                        grid.sample_count(),
                    );
                    decoded_spans.extend([
                        grid.dimensions_span,
                        grid.descriptor_span,
                        grid.samples_span,
                    ]);
                } else {
                    count(details, "unknown_consumer_roles", 1);
                }
            }
            let decoded_bytes = union_bytes_within(bank.trailing_data, decoded_spans);
            count(details, "decoded_trailing_bytes", decoded_bytes);
            count(
                details,
                "opaque_trailing_bytes",
                bank.trailing_data.len() - decoded_bytes,
            );
            bank.records.len()
        }
        11 => parse_mission_placements(source, region)?.entries.len(),
        12 => parse_terrain_bank(source, region)?.entries.len(),
        13 => {
            let bank = parse_constructor_asset_bank(source, region)?;
            count(details, "relocations", bank.prefixes.relocations.len());
            for asset in &bank.assets {
                if let ConstructorPayloadView::SharedActor(actor) = &asset.payload {
                    count(details, "damage_responses", actor.damage_responses.len());
                    if let Some(derived) = &actor.derived {
                        count(details, "derived_actor_prefixes", 1);
                        count(
                            details,
                            "constructor_sound_bindings",
                            derived.sound_bindings.len(),
                        );
                    }
                }
                count(
                    details,
                    "constructor_dispatches",
                    usize::from(asset.dispatch.is_some()),
                );
                count(
                    details,
                    "initial_model_bindings",
                    usize::from(asset.model_binding.is_some()),
                );
                count(
                    details,
                    match asset.payload {
                        ConstructorPayloadView::SharedActor(_) => "shared_actor_prefixes",
                        ConstructorPayloadView::Flags(_) => "flags_only_prefixes",
                        ConstructorPayloadView::TerrainLight(_) => "terrain_light_prefixes",
                        ConstructorPayloadView::Selector86(_) => "selector86_prefixes",
                        ConstructorPayloadView::Selector83(_) => "selector83_prefixes",
                        ConstructorPayloadView::Selector45(_) => "selector45_prefixes",
                        ConstructorPayloadView::Selector46(_) => "selector46_prefixes",
                        ConstructorPayloadView::FlagsWords(_) => "flags_words_prefixes",
                        ConstructorPayloadView::Goal(_) => "goal_prefixes",
                        ConstructorPayloadView::PointTable16(_) => "point_table_prefixes",
                        ConstructorPayloadView::Opaque => "opaque_class_payloads",
                    },
                    1,
                );
            }
            bank.assets.len()
        }
        14 => {
            let bank = parse_world_instance_bank(source, region)?;
            count(
                details,
                "placements",
                bank.worlds.entries.iter().map(|w| w.placements.len()).sum(),
            );
            count(
                details,
                "instance_commands",
                bank.instances
                    .iter()
                    .flat_map(|world| &world.placements)
                    .map(|p| p.commands.len())
                    .sum(),
            );
            count(details, "relocations", bank.worlds.relocations.len());
            bank.worlds.entries.len()
        }
        15 => parse_mission_parameters(source, region)?.entries.len(),
        _ => unreachable!("validated HandlerId"),
    })
}

/// Coverage counts source bytes once, even when records alias a buffer.
fn union_bytes_within(bounds: ByteSpan, spans: impl IntoIterator<Item = ByteSpan>) -> usize {
    let mut spans: Vec<_> = spans
        .into_iter()
        .filter_map(|span| {
            let start = span.start.max(bounds.start);
            let end = span.end.min(bounds.end);
            (start < end).then_some((start, end))
        })
        .collect();
    spans.sort_unstable();
    let mut end = bounds.start;
    let mut total = 0;
    for (start, next_end) in spans {
        if next_end > end {
            total += next_end - start.max(end);
            end = next_end;
        }
    }
    total
}

pub fn build(data_root: &Path) -> Result<CoverageReport, Box<dyn Error>> {
    let inventory = inventory::build(data_root)?;
    let preload = Preload::parse(&fs::read(data_root.join("PreLoad.dat"))?)?;
    let mut handlers: Vec<_> = (0..16).map(handler_coverage).collect();
    let mut models = ModelAudit {
        variants: BTreeMap::new(),
        summary: ModelCoverage::default(),
    };
    for file in inventory.files {
        if file.regions.is_empty() {
            continue;
        }
        let source = fs::read(data_root.join("Overlay").join(&file.name))?;
        let requests = preload
            .leaf(file.key.level)
            .expect("inventory validated leaf")
            .requests(file.key.request_kind());
        for region in &file.regions {
            let coverage = &mut handlers[usize::from(region.handler.get())];
            coverage.regions += 1;
            coverage.framed_bytes += region.span.len();
            coverage.requested_entries += region.requested_count as usize;
            coverage.parsed_entries += audit_region(
                &source,
                region,
                requests,
                &file.key,
                &file.name,
                coverage,
                &mut models,
            )
            .map_err(|error| format!("{} handler {}: {error}", file.name, region.handler.get()))?;
        }
    }
    models.summary.unique_slot_content_variants = models.variants.len();
    models.summary.global_slots = models
        .variants
        .keys()
        .map(|(slot, _)| slot)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    Ok(CoverageReport {
        summary: CoverageSummary {
            schema_version: 2,
            scope: "installed_preload_and_ovl",
            interpretation: "Framed bytes and parsed records are not semantic completion. Model counts separate physical registrations from unique slot/content variants; complete_root_mesh excludes runtime child/effect/presentation claims. Non-OVL media and executable tables are outside this census.",
            inventory: inventory.summary, handlers, models: models.summary,
        },
        model_variants: models.variants.into_values().collect(),
    })
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
