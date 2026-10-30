use super::*;
use std::path::PathBuf;

#[test]
fn retail_readability_distinguishes_framing_raw_records_and_partial_models() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let report = build(&root).unwrap();
    let s = &report.summary;
    assert_eq!(s.schema_version, 2);
    assert_eq!(s.inventory.files, 1932);
    assert_eq!(s.inventory.input_bytes, s.inventory.covered_bytes);
    assert_eq!(
        s.handlers
            .iter()
            .map(|h| h.framed_bytes as u64)
            .sum::<u64>(),
        s.inventory.covered_bytes
    );
    assert_eq!(s.handlers[4].knowledge, Knowledge::TypedAssetData);
    assert_eq!(s.handlers[4].parsed_entries, 5);
    assert_eq!(s.handlers[4].details["glyph_metrics"], 1152);
    assert_eq!(s.handlers[13].requested_entries, 162);
    assert_eq!(s.handlers[14].requested_entries, 33);
    for h in [13, 14] {
        assert_eq!(
            s.handlers[h].parsed_entries,
            s.handlers[h].requested_entries
        );
        assert_eq!(s.handlers[h].knowledge, Knowledge::PartialRecordSemantics);
    }
    assert_eq!(s.handlers[14].details["placements"], 4486);
    assert_eq!(s.handlers[14].details["instance_commands"], 11476);
    assert_eq!(s.handlers[13].details["damage_responses"], 1230);
    assert_eq!(s.handlers[13].details["derived_actor_prefixes"], 52);
    assert_eq!(s.handlers[13].details["constructor_sound_bindings"], 208);
    assert_eq!(s.handlers[13].details["constructor_dispatches"], 162);
    assert_eq!(s.handlers[13].details["initial_model_bindings"], 155);
    assert_eq!(s.handlers[13].details["shared_actor_prefixes"], 123);
    assert_eq!(s.handlers[13].details["flags_only_prefixes"], 23);
    assert_eq!(s.handlers[13].details["selector86_prefixes"], 1);
    assert_eq!(s.handlers[13].details["selector83_prefixes"], 3);
    assert_eq!(s.handlers[13].details["selector45_prefixes"], 3);
    assert_eq!(s.handlers[13].details["selector46_prefixes"], 2);
    assert_eq!(s.handlers[13].details["flags_words_prefixes"], 2);
    assert_eq!(s.handlers[13].details["goal_prefixes"], 2);
    assert_eq!(s.handlers[13].details["terrain_light_prefixes"], 2);
    assert_eq!(s.handlers[13].details["point_table_prefixes"], 1);
    assert_eq!(
        s.handlers[13]
            .details
            .get("opaque_class_payloads")
            .copied()
            .unwrap_or(0),
        0
    );
    assert_eq!(s.handlers[10].details["opaque_trailing_bytes"], 0);
    assert_eq!(s.handlers[10].details["decoded_trailing_bytes"], 288020);
    assert_eq!(s.handlers[10].details["sample_grids"], 15);
    assert_eq!(s.handlers[10].details["sample_bytes"], 287700);
    assert_eq!(s.handlers[10].details["eight_word_samples"], 13700);
    assert_eq!(s.handlers[10].details["mask_samples"], 13700);
    assert_eq!(s.handlers[10].details["material_index_samples"], 54800);
    assert_eq!(s.models.physical_entries, 5142);
    assert_eq!(s.models.global_slots, 760);
    assert_eq!(s.models.unique_slot_content_variants, 849);
    for (auxiliary, entries) in [
        (&s.models.physical_auxiliary, s.models.physical_entries),
        (
            &s.models.unique_variant_auxiliary,
            s.models.unique_slot_content_variants,
        ),
    ] {
        assert_eq!(
            auxiliary.active_programs + auxiliary.inactive_entries,
            entries
        );
        assert!(auxiliary.active_programs > 0);
        assert_eq!(auxiliary.inactive_entries_with_auxiliary_bytes, 0);
        assert_eq!(
            auxiliary.opcode_instructions.values().sum::<usize>(),
            auxiliary.instructions
        );
        assert!(!auxiliary.opcode_instructions.contains_key(&0x88));
    }
    assert_eq!(s.models.physical_auxiliary.active_programs, 2526);
    assert_eq!(s.models.physical_auxiliary.inactive_entries, 2616);
    assert_eq!(s.models.physical_auxiliary.instructions, 20253);
    assert_eq!(s.models.unique_variant_auxiliary.active_programs, 354);
    assert_eq!(s.models.unique_variant_auxiliary.inactive_entries, 495);
    assert_eq!(s.models.unique_variant_auxiliary.instructions, 2859);
    assert_eq!(
        s.models.physical_auxiliary.opcode_instructions,
        BTreeMap::from([
            (0x05, 6),
            (0x07, 9250),
            (0x18, 242),
            (0x2f, 10),
            (0x30, 10),
            (0x89, 277),
            (0x8c, 50),
            (0x8d, 50),
            (0x8e, 7404),
            (0x8f, 436),
            (0x95, 2518),
        ])
    );
    assert_eq!(
        s.models.unique_variant_auxiliary.opcode_instructions,
        BTreeMap::from([
            (0x05, 3),
            (0x07, 1365),
            (0x18, 18),
            (0x2f, 5),
            (0x30, 5),
            (0x89, 75),
            (0x8c, 11),
            (0x8d, 11),
            (0x8e, 932),
            (0x8f, 85),
            (0x95, 349),
        ])
    );
    assert_eq!(s.models.physical_diagnostics["partial_mesh"], 1765);
    assert_eq!(s.models.unique_variant_diagnostics["partial_mesh"], 290);
    assert_eq!(s.models.physical_diagnostics["complete_root_mesh"], 2369);
    assert_eq!(
        s.models.unique_variant_diagnostics["complete_root_mesh"],
        372
    );
    assert_eq!(
        s.models.unique_variant_diagnostics["runtime_dependent_geometry"],
        41
    );
    assert_eq!(
        s.models.physical_diagnostics["runtime_dependent_geometry"],
        290
    );
    assert!(!s
        .models
        .unique_variant_diagnostics
        .contains_key("nested_program"));
    assert!(!s
        .models
        .unique_variant_diagnostics
        .contains_key("unresolved_command_effects"));
    assert_eq!(
        report
            .model_variants
            .iter()
            .map(|v| v.occurrences.len())
            .sum::<usize>(),
        5142
    );
    for variant in &report.model_variants {
        assert!(!variant.occurrences.is_empty());
        if !matches!(variant.diagnostic, "partial_mesh" | "complete_root_mesh") {
            assert!(variant.reason.is_some());
        }
        for occurrence in &variant.occurrences {
            assert!(!occurrence.content_span.is_empty());
            assert!(root.join("Overlay").join(&occurrence.ovl).is_file());
        }
    }
}

#[test]
fn exact_slot_content_identity_retains_repeated_provenance_and_real_variants() {
    // Use real registrations to catch a census that accidentally deduplicates
    // by global slot only or treats file padding as a distinct logical asset.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let report = build(&root).unwrap();
    let mut variants = BTreeMap::new();
    for v in report.model_variants {
        count(&mut variants, v.global_slot, 1);
    }
    assert_eq!(variants.values().filter(|&&n| n > 1).count(), 16);
    assert_eq!(variants.values().filter(|&&n| n == 1).count(), 744);
}

#[test]
fn aliased_sample_buffers_do_not_hide_uncovered_trailing_bytes() {
    assert_eq!(
        union_bytes_within(
            ByteSpan::new(10, 30),
            [
                ByteSpan::new(0, 15),
                ByteSpan::new(12, 20),
                ByteSpan::new(12, 20),
                ByteSpan::new(25, 99),
                ByteSpan::new(15, 15),
            ]
        ),
        15
    );
}
