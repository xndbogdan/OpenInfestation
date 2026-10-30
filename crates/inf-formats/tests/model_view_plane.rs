use std::{fs, path::Path};

use inf_formats::{
    analyze_model_diagnostic, parse_model_bank, parse_overlay, ByteSpan,
    ModelDiagnosticCompleteness, ModelDiagnosticGeometry, ModelDiagnosticProjection,
    ModelDiagnosticRequest, ModelDiagnosticView, ModelDynamicDependency, ModelEntry,
    ModelInstruction, ModelInstructionKind, ModelRawCommand, OverlayKey, Preload,
};

fn signs() -> (Vec<u8>, Vec<ModelEntry>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let source = fs::read(root.join("Overlay/15.ovl")).expect("retail source required");
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let key = OverlayKey::parse_filename("15.ovl").unwrap();
    let requests = preload.leaf(15).unwrap().requests(key.request_kind());
    let layout = parse_overlay(&source, requests).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|r| r.handler.get() == 8)
        .unwrap();
    let entries = parse_model_bank(&source, region, requests.get(8).unwrap(), &key)
        .unwrap()
        .entries
        .into_iter()
        .filter(|e| (560..=562).contains(&e.identity.global_index))
        .collect();
    (source, entries)
}

fn explicit(entry: &ModelEntry, source: &[u8], eye_root: [i32; 3]) -> ModelDiagnosticProjection {
    analyze_model_diagnostic(ModelDiagnosticRequest {
        entry,
        view: ModelDiagnosticView::ExplicitEye {
            source,
            eye_root,
            coordinate_shift: 16,
        },
    })
}

fn ready(projection: ModelDiagnosticProjection) -> ModelDiagnosticGeometry {
    let ModelDiagnosticProjection::Ready(geometry) = projection else {
        panic!("expected view-conditioned geometry, got {projection:?}")
    };
    geometry
}

fn assert_unknown(projection: ModelDiagnosticProjection) {
    match projection {
        ModelDiagnosticProjection::Unavailable(unavailable) => {
            assert!(unavailable
                .unresolved_dependencies
                .contains(&ModelDynamicDependency::ViewPlane));
        }
        ModelDiagnosticProjection::Ready(geometry) => {
            assert!(geometry
                .unresolved_dependencies
                .contains(&ModelDynamicDependency::ViewPlane));
            assert!(geometry.view_plane_decisions.is_empty());
        }
    }
}

#[test]
fn retail_signs_select_opposite_quads_from_literal_command_vectors() {
    let (source, entries) = signs();
    assert_eq!(entries.len(), 3);
    for entry in entries {
        assert_unknown(analyze_model_diagnostic(
            ModelDiagnosticRequest::unconditioned(&entry),
        ));
        let material = 1932 - entry.identity.global_index;
        assert_eq!(entry.normals.len(), 1);
        assert_eq!(
            [entry.normals[0].x, entry.normals[0].y, entry.normals[0].z],
            [0, 0, 4096]
        );
        for (z, expected_slots, jumps) in [
            (-1024, [24, 10, 6, 4], [false, true]),
            (0, [24, 10, 6, 4], [false, true]),
            (1024, [22, 20, 16, 18], [true, false]),
        ] {
            let geometry = ready(explicit(&entry, &source, [-128, 256, z]));
            assert_eq!(
                geometry.completeness,
                ModelDiagnosticCompleteness::CompleteMesh
            );
            assert_eq!(geometry.ignored_edge_commands, 4);
            assert!(geometry.unresolved_dependencies.is_empty());
            assert_eq!(geometry.faces.len(), 1);
            assert_eq!(
                geometry.faces[0]
                    .iter()
                    .map(|&i| geometry.vertices[i].slot.raw())
                    .collect::<Vec<_>>(),
                expected_slots
            );
            assert_eq!(
                geometry.face_presentation[0].material.global_id,
                material as i16
            );
            assert_eq!(geometry.view_plane_decisions.len(), 2);
            for (decision, jump) in geometry.view_plane_decisions.iter().zip(jumps) {
                assert_eq!(decision.raw_vector, [2, 226, material as i16]);
                assert_eq!(decision.effective_vector, [2, 226, i32::from(material)]);
                assert_eq!(
                    decision.vector_source_span,
                    ByteSpan::new(entry.normals_span.start + 12, entry.normals_span.start + 18)
                );
                assert_eq!(decision.world_point, [-128, 256, 0]);
                assert_eq!(decision.eye_root, [-128, 256, z]);
                assert_eq!(decision.coordinate_shift, 16);
                assert_eq!(decision.dot_product, -z * i32::from(material));
                assert_eq!(decision.jump_taken, jump);
            }
        }
        // Explicit inspection does not mutate the intrinsic, unconditioned result.
        assert_unknown(analyze_model_diagnostic(
            ModelDiagnosticRequest::unconditioned(&entry),
        ));
    }
}

#[test]
fn command_vector_tilt_is_not_replaced_by_face_normal_or_winding() {
    let (source, entries) = signs();
    let geometry = ready(explicit(&entries[0], &source, [-128, 257, 0]));
    // Geometrical [0,0,4096] would yield zero. Native command bytes yield -226.
    assert_eq!(geometry.view_plane_decisions[0].dot_product, -226);
    assert!(geometry.view_plane_decisions[0].jump_taken);
}

#[test]
fn incomplete_bytes_unknown_world_slots_and_invalid_indexes_remain_unresolved() {
    let (source, entries) = signs();
    let entry = &entries[0];
    assert_unknown(explicit(
        entry,
        &source[..entry.normals_span.start + 17],
        [0; 3],
    ));

    let mut unknown_world = entry.clone();
    unknown_world.header.vertex_ranges = [26; 4];
    assert_unknown(explicit(&unknown_world, &source, [0; 3]));

    for (argument, value) in [(1, 0xffff), (1, 25), (2, 0x7fff), (2, 0x8000)] {
        let mut invalid = entry.clone();
        for instruction in &mut invalid.program.instructions {
            if let ModelInstructionKind::UnresolvedDynamic(command) = &mut instruction.kind {
                command.arguments[argument] = value;
            }
        }
        assert_unknown(explicit(&invalid, &source, [0; 3]));
    }
}

#[test]
fn signed_vector_index_can_name_proven_prefix_bytes_within_entry() {
    let (source, entries) = signs();
    let mut entry = entries[0].clone();
    for instruction in &mut entry.program.instructions {
        if let ModelInstructionKind::UnresolvedDynamic(command) = &mut instruction.kind {
            command.arguments[2] = 0xffff;
        }
    }
    let geometry = ready(explicit(&entry, &source, [-128, 257, 0]));
    assert_eq!(geometry.view_plane_decisions[0].raw_vector, [-128, 256, 0]);
    assert_eq!(geometry.view_plane_decisions[0].vector_index, -1);
    assert_eq!(
        geometry.view_plane_decisions[0].vector_source_span,
        ByteSpan::new(entry.normals_span.start - 6, entry.normals_span.start)
    );
}

#[test]
fn unsupported_basis_mutations_do_not_use_the_root_identity_fallback() {
    let (source, entries) = signs();
    for opcode in [0x03, 0x04, 0x0f, 0x10, 0x1c, 0xee, 0xef] {
        let mut entry = entries[0].clone();
        entry.program.instructions.insert(
            0,
            ModelInstruction {
                span: ByteSpan::new(entry.command_span.start - 2, entry.command_span.start),
                raw_command: opcode,
                kind: ModelInstructionKind::Opaque(ModelRawCommand { arguments: vec![] }),
            },
        );
        assert_unknown(explicit(&entry, &source, [0; 3]));
    }
}

#[test]
fn backward_or_invalid_branch_target_is_contained_without_looping() {
    let (source, entries) = signs();
    for jump in [0xffff, 0x8000, 0x7fff, 1] {
        let mut entry = entries[0].clone();
        if let ModelInstructionKind::UnresolvedDynamic(command) =
            &mut entry.program.instructions[0].kind
        {
            command.arguments[0] = jump;
        }
        assert!(matches!(
            explicit(&entry, &source, [-128, 256, 1024]),
            ModelDiagnosticProjection::Unavailable(_)
        ));
    }
}
