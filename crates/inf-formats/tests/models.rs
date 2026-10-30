use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{
    analyze_model_diagnostic, decode_model_program, parse_model_bank, parse_overlay, ByteSpan,
    HandlerFraming, HandlerId, HandlerRegion, HandlerRequest, ModelAxis,
    ModelDiagnosticCompleteness, ModelDiagnosticFaceFlags, ModelDiagnosticFaceLighting,
    ModelDiagnosticOmissionKind, ModelDiagnosticProjection, ModelDiagnosticUnavailableKind,
    ModelDynamicDependency, ModelEntry, ModelError, ModelFanVertexAttributes, ModelFlags,
    ModelHeader, ModelIdentity, ModelInstructionKind, ModelMaterialDomain, ModelProgram,
    ModelSubBlock, ModelVectorI16, OverlayKey, OverlayVariant, Preload, ResourceSegment,
};

fn infestation_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn retail_model(ovl: &str, local_index: u16) -> inf_formats::ModelEntry {
    let root = infestation_root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let key = OverlayKey::parse_filename(ovl).unwrap();
    let request = preload
        .leaf(key.level)
        .unwrap()
        .requests(key.request_kind())
        .get(8)
        .unwrap();
    let bytes = fs::read(root.join("Overlay").join(ovl)).unwrap();
    let layout = parse_overlay(
        &bytes,
        preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind()),
    )
    .unwrap();
    let region = layout
        .regions
        .iter()
        .find(|region| region.handler == HandlerId::MODELS)
        .unwrap();
    parse_model_bank(&bytes, region, request, &key)
        .unwrap()
        .entries
        .into_iter()
        .find(|entry| entry.identity.local_index == local_index)
        .unwrap()
}

fn count_nested_programs(program: &ModelProgram, census: &mut [usize; 4]) {
    for instruction in &program.instructions {
        let ModelInstructionKind::NestedProgram(nested) = &instruction.kind else {
            continue;
        };
        census[0] += 1;
        if instruction.raw_command as u8 == 0x05 {
            let literal_rotations = nested
                .arguments
                .get(1..4)
                .is_some_and(|rotations| rotations.iter().all(|value| value & 0x80 == 0));
            census[if literal_rotations { 1 } else { 2 }] += 1;
        } else {
            census[3] += 1;
        }
        count_nested_programs(&nested.program, census);
    }
}

#[test]
fn pinned_retail_diagnostics_follow_root_slot_initialization() {
    let nothing = retail_model("0.ovl", 0);
    let ModelDiagnosticProjection::Unavailable(nothing) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&nothing))
    else {
        panic!("NOTHING must remain a classified archive-empty entry")
    };
    assert_eq!(nothing.kind, ModelDiagnosticUnavailableKind::Empty);
    assert!(nothing.referenced_face_slots.is_empty());
    assert!(nothing.unresolved_dependencies.is_empty());

    let beacon = retail_model("1.ovl", 0);
    let ModelDiagnosticProjection::Ready(beacon) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&beacon))
    else {
        panic!("BEACON has a conservative repeated static body projection")
    };
    assert_eq!(beacon.vertices.len(), 24);
    assert_eq!(beacon.faces.len(), 14);
    assert_eq!(beacon.face_presentation.len(), beacon.faces.len());
    assert_eq!(
        beacon.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert_eq!(
        beacon
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::ConditionalFaces)
            .map(|omission| omission.count),
        Some(28)
    );
    assert_eq!(
        beacon
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        Some(1)
    );
    assert!(beacon
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::PackedValueCallback));
    assert!(beacon
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::RegisterFile));
    let repeated_body = beacon
        .face_presentation
        .iter()
        .find(|presentation| {
            matches!(
                presentation.source_flags,
                ModelDiagnosticFaceFlags::Face(flags) if flags.raw() == 0x0500
            ) && matches!(
                presentation.source_lighting,
                ModelDiagnosticFaceLighting::Shared(slot) if slot.raw() == 10
            )
        })
        .expect("BEACON's previously pinned repeated body face");
    assert_eq!(repeated_body.source_opcode, 0xe4);
    assert_eq!(
        repeated_body.material.domain,
        ModelMaterialDomain::Handler7Material
    );
    assert_eq!(repeated_body.material.global_id, 0);

    let missile = retail_model("1.ovl", 1);
    let ModelDiagnosticProjection::Ready(missile) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&missile))
    else {
        panic!("MISSILE's polygon inputs are initialized separately from its effect state")
    };
    assert_eq!(
        missile.completeness,
        ModelDiagnosticCompleteness::CompleteMesh
    );
    assert_eq!(
        missile
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        None
    );
    assert_eq!(missile.faces.len(), 42);
    assert_eq!(
        missile
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::NonMeshEffects)
            .map(|omission| (omission.count, omission.opcodes.as_slice())),
        Some((2, [0x1b].as_slice()))
    );
    assert!(missile
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::PackedValueCallback));
    assert!(missile
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::RegisterFile));

    let bigdish = retail_model("17.ovl", 10);
    let ModelDiagnosticProjection::Ready(bigdish) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&bigdish))
    else {
        panic!("BIGDISH's faces consume raw projections despite its empty world range")
    };
    assert_eq!(
        bigdish.completeness,
        ModelDiagnosticCompleteness::CompleteMesh
    );
    assert_eq!(
        bigdish
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        None
    );
    assert_eq!(bigdish.faces.len(), 46);
    assert!(bigdish.unresolved_dependencies.is_empty());

    let gunbase_entry = retail_model("1.ovl", 40);
    let gunbase_instance = gunbase_entry
        .program
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            ModelInstructionKind::ModelInstance(instance) => Some(instance),
            _ => None,
        })
        .expect("GUNBASE child instance");
    assert_eq!(gunbase_instance.model.raw(), 43);
    assert!(gunbase_instance.orientation.uses_saved_basis());
    assert_eq!(gunbase_instance.state_words, [4]);
    let ModelDiagnosticProjection::Ready(gunbase) = analyze_model_diagnostic(
        inf_formats::ModelDiagnosticRequest::unconditioned(&gunbase_entry),
    ) else {
        panic!(
            "GUNBASE retains its authored local quad while its callback face/child remain unknown"
        )
    };
    assert_eq!(
        gunbase.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert_eq!(
        gunbase
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        Some(1)
    );
    assert_eq!(gunbase.faces.len(), 1);
    assert_eq!(
        gunbase
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
            .map(|omission| omission.count),
        Some(1)
    );
    assert!(gunbase
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::SavedTransformBasis));
    assert!(gunbase
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::ExternalModelCatalog));

    let gunstrut_entry = retail_model("1.ovl", 41);
    let gunstrut_instance = gunstrut_entry
        .program
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            ModelInstructionKind::ModelInstance(instance) => Some(instance),
            _ => None,
        })
        .expect("GUNSTRUT01 child instance");
    assert_eq!(gunstrut_instance.model.raw(), 44);
    assert!(gunstrut_instance.orientation.uses_saved_basis());
    assert_eq!(gunstrut_instance.state_words, [4, 2]);
    let ModelDiagnosticProjection::Ready(gunstrut) = analyze_model_diagnostic(
        inf_formats::ModelDiagnosticRequest::unconditioned(&gunstrut_entry),
    ) else {
        panic!("GUNSTRUT01 retains its authored shell while its child remains unknown")
    };
    assert_eq!(
        gunstrut.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert_eq!(
        gunstrut
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        None
    );
    assert_eq!(gunstrut.faces.len(), 9);
    assert_eq!(
        gunstrut
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
            .map(|omission| omission.count),
        Some(1)
    );
    assert!(gunstrut
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::SavedTransformBasis));
    assert!(gunstrut
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::ExternalModelCatalog));

    let gunmain = retail_model("1.ovl", 42);
    let parent_import = gunmain
        .program
        .instructions
        .iter()
        .find(|instruction| instruction.raw_command as u8 == 0x2a)
        .expect("GUNMAIN01 parent-slot import");
    let ModelInstructionKind::UnresolvedDynamic(parent_import) = &parent_import.kind else {
        panic!("GUNMAIN01 opcode 0x2A must retain its typed dynamic dependency")
    };
    assert_eq!(parent_import.arguments, [24, 1]);
    assert!(parent_import
        .dependencies
        .contains(&ModelDynamicDependency::ParentVertexSlots));
}

fn diagnostic_entry(command_words: &[u16]) -> ModelEntry {
    ModelEntry {
        identity: ModelIdentity {
            local_index: 0,
            global_index: 0,
        },
        span: ByteSpan::new(0, 0),
        content_span: ByteSpan::new(0, 0),
        header_span: ByteSpan::new(0, 0),
        header: ModelHeader {
            visibility_extent: 0,
            command_word_count: command_words.len() as u16,
            auxiliary_count: 0,
            flags: ModelFlags::from_raw(ModelFlags::OMIT_NAME),
            vertex_slot_count: 6,
            raw_vertex_count: 3,
            shade_slot_count: 2,
            raw_0e: 0,
            reserved_10: 0,
            vertex_ranges: [0, 2, 6, 6],
            reserved_1a: 0,
            shade_ranges: [0, 0],
        },
        auxiliary_span: ByteSpan::new(0, 0),
        auxiliary: Vec::new(),
        auxiliary_program: None,
        vertices_span: ByteSpan::new(0, 0),
        vertices: vec![
            ModelVectorI16 { x: 0, y: 0, z: 0 },
            ModelVectorI16 { x: 10, y: 2, z: 4 },
            ModelVectorI16 { x: 30, y: 6, z: 8 },
        ],
        normals_span: ByteSpan::new(0, 0),
        normals: Vec::new(),
        command_span: ByteSpan::new(0, command_words.len() * 2),
        program: decode_model_program(command_words).unwrap(),
        name: None,
        padding: ByteSpan::new(0, 0),
    }
}

#[test]
fn diagnostic_keeps_faces_after_equal_branch_and_fallthrough_targets() {
    for command in [0x0016, 0x0116, 0xff16] {
        let entry = diagnostic_entry(&[
            command, 3, 0x80, 0xc0, // callback vs unknown register; both paths coincide
            0x00d8, 7, 0, 0, 2, 4, // shared-light triangle
            0,
        ]);
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
        else {
            panic!("equal branch targets cannot hide the following polygon")
        };
        assert_eq!(geometry.faces.len(), 1);
        assert_eq!(
            geometry.completeness,
            ModelDiagnosticCompleteness::CompleteMesh
        );
        assert!(geometry.omitted_effects.is_empty());
        assert!(geometry
            .unresolved_dependencies
            .contains(&ModelDynamicDependency::PackedValueCallback));
        assert!(geometry
            .unresolved_dependencies
            .contains(&ModelDynamicDependency::RegisterFile));
    }

    let guarded = diagnostic_entry(&[
        0x0016, 13, 0x80, 0xc0, // unknown outer condition skips the remaining program
        0x0016, 3, 0x80, 0xc0, // equal targets do not remove the outer condition
        0x00d8, 7, 0, 0, 2, 4, 0,
    ]);
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&guarded))
    else {
        panic!("the enclosing unresolved condition must still hide its face")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
    );
    assert_eq!(
        unavailable.omitted_effects[0].kind,
        ModelDiagnosticOmissionKind::ConditionalFaces
    );
    assert_eq!(unavailable.omitted_effects[0].count, 1);
}

#[test]
fn diagnostic_register_shift_uses_native_dword_count_before_low_word_store() {
    for (shift, expected) in [(0x11, 0), (0x15, 1)] {
        // Packed 0x11 is sixteen; 0x15 is thirty-two. Native DWORD shifts
        // produce zero and one respectively before their low WORD is stored.
        let entry = diagnostic_entry(&[
            0x00f5, 0, 1, shift, 0x0016, 9, 0xc0, expected, 0x00d8, 7, 0, 0, 2, 4, 0,
        ]);
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
        else {
            panic!("the native shift result must retain the following face")
        };
        assert_eq!(geometry.faces.len(), 1);
        assert_eq!(
            geometry.completeness,
            ModelDiagnosticCompleteness::CompleteMesh
        );
    }
}

#[test]
fn diagnostic_view_plane_branches_retain_their_camera_dependency() {
    let entry = diagnostic_entry(&[0x000b, 9, 0, 2, 0x00d8, 7, 0, 0, 2, 4, 0]);
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("an unknown view plane must not select a face")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
    );
    assert_eq!(
        unavailable.unresolved_dependencies,
        [ModelDynamicDependency::ViewPlane]
    );

    for global in [560, 561, 562] {
        let root = infestation_root();
        let bytes = fs::read(root.join("Overlay/15.ovl")).unwrap();
        let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
        let key = OverlayKey::parse_filename("15.ovl").unwrap();
        let requests = preload.leaf(15).unwrap().requests(key.request_kind());
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 8)
            .unwrap();
        let bank = parse_model_bank(&bytes, region, requests.get(8).unwrap(), &key).unwrap();
        let entry = bank
            .entries
            .iter()
            .find(|entry| entry.identity.global_index == global)
            .unwrap();
        let ModelDiagnosticProjection::Unavailable(unavailable) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(entry))
        else {
            panic!("zone signs require the original camera-plane selection")
        };
        assert!(unavailable
            .unresolved_dependencies
            .contains(&ModelDynamicDependency::ViewPlane));
    }
}

#[test]
fn diagnostic_executes_proven_midpoint_vertex_operation() {
    let entry = diagnostic_entry(&[
        0x0323, 0, 2, 4, // write world and projection slot 0 = midpoint(slot 2, slot 4)
        0x00d8, 7, 0, 0, 2, 4, // shared-light triangle
        0,
    ]);
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("deterministic midpoint must make the face statically projectable")
    };
    assert_eq!(geometry.faces, [vec![0, 1, 2]]);
    assert_eq!(geometry.vertices[0].slot.raw(), 0);
    assert_eq!(geometry.vertices[0].position, [20, 4, 6]);
    assert_eq!(geometry.face_presentation[0].source_opcode, 0xd8);
    assert_eq!(geometry.face_presentation[0].material.global_id, 7);
    assert!(matches!(
        geometry.face_presentation[0].source_lighting,
        ModelDiagnosticFaceLighting::Shared(slot) if slot.raw() == 0
    ));
}

#[test]
fn diagnostic_projects_static_handler_05_and_restores_only_transform_state() {
    let mut entry = diagnostic_entry(&[
        0x0005, // nested transform
        0,      // origin slot
        0x4000, 0, 0, // FUN_00491530 / 00491290 / 004917D0 rotations
        0, // projection start
        0, 6, 6, // static start / mirror start / end
        6, 0, 0, 0, // projection end / shade words
        0x00d8, 7, 0, 0, 2, 4, // nested face
        0, // nested terminator
        0x0320, 0, // root-scope world/projection write after transform restoration
        0x00d8, 7, 0, 0, 2, 4, // root face
        0,
    ]);
    entry.vertices[0] = ModelVectorI16 { x: 1, y: 3, z: 5 };
    entry.header.vertex_ranges = [0, 6, 6, 6];

    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("literal Handler-8 transform must be statically projectable")
    };
    assert_eq!(geometry.faces.len(), 2);
    let face_vertices = geometry
        .faces
        .iter()
        .map(|face| {
            face.iter()
                .map(|&index| {
                    let vertex = geometry.vertices[index];
                    (vertex.slot.raw(), vertex.position)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        face_vertices[0],
        [(0, [6, 6, 4]), (2, [5, 5, -5]), (4, [9, 9, -25])]
    );
    assert_eq!(
        face_vertices[1],
        [(0, [1, 3, 5]), (2, [5, 5, -5]), (4, [9, 9, -25])]
    );
    assert_eq!(
        geometry.completeness,
        ModelDiagnosticCompleteness::CompleteMesh
    );
    assert!(geometry.unresolved_dependencies.is_empty());
}

#[test]
fn diagnostic_invalidates_handler_05_slots_for_callback_rotation() {
    let mut entry = diagnostic_entry(&[
        0x0005, 0, 0x0080, 0, 0, // origin plus callback/zero/zero rotations
        0, 0, 6, 6, 0, 0, 0, 0, // range and unused words
        0x00d8, 7, 0, 0, 2, 4, // nested face
        0, // nested terminator
        0x00d8, 7, 0, 0, 2, 4, // stale slots must not leak after restoration
        0,
    ]);
    entry.header.vertex_ranges = [0, 6, 6, 6];

    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("callback-driven nested geometry cannot have a static projection")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::PackedValueCallback));
    assert_eq!(
        unavailable
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::UnresolvedFaceSlots)
            .map(|omission| omission.count),
        Some(2)
    );
}

#[test]
fn empty_dynamic_nested_scope_keeps_later_root_faces_runtime_dependent() {
    let mut entry = diagnostic_entry(&[
        0x00f5, 1, 0x0080, 0x400a, // callback-derived register 1
        0x0005, 0, 0x00c1, 0, 0, // origin plus register/zero/zero rotations
        0, 6, 6, 6, 6, 0, 0, 0, // only the projection bank is initialized
        0, // empty nested program still writes callback-dependent projections
        0x00d8, 7, 0, 0, 2, 4, // root face reads those persistent slots
        0,
    ]);
    entry.header.vertex_ranges = [0, 6, 6, 6];
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("an empty nested body does not make its projection writes static")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
    );
    assert_eq!(
        unavailable.unresolved_dependencies,
        [
            ModelDynamicDependency::PackedValueCallback,
            ModelDynamicDependency::RegisterFile
        ]
    );
    assert!(!unavailable.reason.contains("only nested"));
    assert_eq!(unavailable.omitted_effects.len(), 1);
    assert_eq!(unavailable.omitted_effects[0].count, 1);
    assert_eq!(
        unavailable.omitted_effects[0].kind,
        ModelDiagnosticOmissionKind::UnresolvedFaceSlots
    );
}

#[test]
fn diagnostic_rejects_handler_05_range_beyond_authored_vertices() {
    let mut entry = diagnostic_entry(&[
        0x0005, 0, 0, 0, 0, // origin plus literal rotations
        0, 0, 6, 6, 0, 0, 0, 0, // range and unused words
        0x00d8, 7, 0, 0, 2, 4, // nested face
        0, 0,
    ]);
    entry.header.vertex_ranges = [0, 2, 2, 6];
    entry.vertices.truncate(2);
    entry.header.raw_vertex_count = 2;

    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("a nested range beyond the authored vertex pool must not be projected")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::NestedProgram
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::TransformState));
}

#[test]
fn diagnostic_keeps_handler_09_explicitly_unavailable() {
    let mut entry = diagnostic_entry(&[
        0x0009, 0, 2, 4, 0, // origin/basis slots plus unused word
        0, 6, 6, 0, 0, 0, 0, // range and unused words
        0x00d8, 7, 0, 0, 2, 4, // nested face
        0, 0,
    ]);
    entry.header.vertex_ranges = [0, 6, 6, 6];
    let ModelInstructionKind::NestedProgram(nested) = &entry.program.instructions[0].kind else {
        panic!("opcode 0x09 must retain its nested program")
    };
    assert_eq!(
        nested.dependencies,
        [ModelDynamicDependency::TransformState]
    );

    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("opcode 0x09 basis construction is not yet proven diagnostic math")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::NestedProgram
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::TransformState));
}

#[test]
fn diagnostic_projects_raw_inputs_without_fabricating_world_slots() {
    let mut entry = diagnostic_entry(&[
        0x04d8, 7, 0, 0, 2, 4, // original and mirrored raw projections
        0x0223, 0, 2, 4, // projection-only midpoint reads uninitialized world sources
        0x00d8, 7, 0, 0, 2, 4, // must not reuse stale projection at destination zero
        0,
    ]);
    entry.header.vertex_ranges = [0, 0, 0, 6];
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("the first faces have proven raw projection sources")
    };
    assert_eq!(geometry.faces.len(), 2);
    assert_eq!(
        geometry.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert_eq!(
        geometry.omitted_effects[0].kind,
        ModelDiagnosticOmissionKind::UnresolvedFaceSlots
    );
    assert_eq!(geometry.omitted_effects[0].count, 1);
    assert_eq!(
        geometry
            .vertices
            .iter()
            .find(|v| v.slot.raw() == 3)
            .unwrap()
            .position,
        [-10, 2, 4]
    );
}

#[test]
fn diagnostic_vertex_modes_keep_world_and_projection_writes_independent() {
    for (command, first, second) in [
        (0x0123, [0, 0, 0], [25, 5, 7]), // world-only changes the later source
        (0x0223, [20, 4, 6], [15, 3, 4]), // projection-only preserves the later world source
        (0x0323, [20, 4, 6], [25, 5, 7]), // both writes
    ] {
        let entry =
            diagnostic_entry(&[command, 0, 2, 4, 0x0223, 2, 0, 4, 0x00d8, 7, 0, 0, 2, 4, 0]);
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
        else {
            panic!("proven independent writes must retain the face")
        };
        let face = &geometry.faces[0];
        assert_eq!(geometry.vertices[face[0]].position, first);
        assert_eq!(geometry.vertices[face[1]].position, second);
    }

    let entry = diagnostic_entry(&[
        0x0a23, 0, 2, 4, // projection-only, both original and mirrored sources
        0x04d8, 7, 0, 0, 2, 4, 0,
    ]);
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("mirrored projection writes have proven world sources")
    };
    assert_eq!(geometry.vertices[geometry.faces[0][0]].position, [20, 4, 6]);
    assert_eq!(
        geometry.vertices[geometry.faces[1][0]].position,
        [-20, 4, 6]
    );
}

#[test]
fn diagnostic_projection_only_write_cannot_supply_a_nested_attachment_origin() {
    let mut entry = diagnostic_entry(&[
        0x0223, 0, 2, 4, // projected midpoint; world slot zero is still unknown
        0x0005, 0, 0, 0, 0, // nested attachment consumes world slot zero
        0, 0, 6, 6, 6, 0, 0, 0, 0x00d8, 7, 0, 0, 2, 4, 0, 0,
    ]);
    entry.header.vertex_ranges = [2, 2, 6, 6];
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("a proven projection input is not an initialized attachment origin")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::NestedProgram
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::TransformState));
}

#[test]
fn diagnostic_parent_import_preserves_native_full_slot_copy_custody() {
    // A nominal world-only parent import also replaces projection bytes.
    let entry = diagnostic_entry(&[0x012a, 0, 1, 0x00d8, 7, 0, 0, 2, 4, 0]);
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("a parent slot copy must not retain stale projected geometry")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::ParentVertexSlots));

    // A nominal projection-only parent import also replaces world XYZ.
    let entry = diagnostic_entry(&[0x022a, 0, 1, 0x0223, 0, 0, 2, 0x00d8, 7, 0, 0, 2, 4, 0]);
    assert!(matches!(
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry)),
        ModelDiagnosticProjection::Unavailable(_)
    ));

    // Null-parent mirror-only projection first writes the original origin.
    // Unknown parent custody cannot retain the original projection either.
    let entry = diagnostic_entry(&[0x082a, 0, 1, 0x00d8, 7, 0, 0, 2, 4, 0]);
    assert!(matches!(
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry)),
        ModelDiagnosticProjection::Unavailable(_)
    ));

    // Null-parent mirror-only world writes do not touch the original
    // projection; the nonnull-parent path touches only the mirrored slot.
    let entry = diagnostic_entry(&[0x042a, 0, 1, 0x00d8, 7, 0, 0, 2, 4, 0]);
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("the original projection is unchanged on both native parent paths")
    };
    assert_eq!(geometry.faces.len(), 1);
}

#[test]
fn diagnostic_interpolation_uses_native_even_steps_and_unknown_projection_writes_clear() {
    let mut entry = diagnostic_entry(&[
        0x0226, 0, 2, 4, 0x8000, // weight 1/2: +3 gives +0, not +1
        0x00d8, 7, 0, 0, 2, 4, 0x022d, 0,
        0, // unknown custom callback replaces only projection input
        0x00d8, 7, 0, 0, 2, 4, 0,
    ]);
    entry.vertices[1] = ModelVectorI16 { x: 10, y: 0, z: 0 };
    entry.vertices[2] = ModelVectorI16 { x: 13, y: 0, z: 0 };
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("the first interpolated face remains proven")
    };
    assert_eq!(geometry.faces.len(), 1);
    assert_eq!(geometry.vertices[geometry.faces[0][0]].position, [10, 0, 0]);
    assert_eq!(
        geometry.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert!(geometry
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::CustomVertexCallback));
}

#[test]
fn retail_projection_only_models_have_complete_authored_polygon_topology() {
    for (source, local, expected_vertices, expected_faces) in [
        ("0.ovl", 1, 36, 30), // mark: mirrored raw range
        ("1.ovl", 2, 7, 3),   // ammobox: even raw range only
    ] {
        let entry = retail_model(source, local);
        assert_eq!(entry.header.vertex_ranges[0], entry.header.vertex_ranges[2]);
        assert!(entry
            .program
            .instructions
            .iter()
            .all(|i| i.raw_command as u8 == 0xe4));
        let ModelDiagnosticProjection::Ready(geometry) =
            analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
        else {
            panic!("raw-only native face inputs must be available")
        };
        assert_eq!(geometry.vertices.len(), expected_vertices);
        assert_eq!(geometry.faces.len(), expected_faces);
        assert_eq!(
            geometry.completeness,
            ModelDiagnosticCompleteness::CompleteMesh
        );
        assert!(geometry.unresolved_dependencies.is_empty());
        assert!(geometry.omitted_effects.is_empty());
        for vertex in geometry.vertices {
            let raw = entry.vertices[usize::from(vertex.slot.raw() / 2)];
            assert_eq!(
                vertex.position,
                [
                    if vertex.slot.raw() & 1 == 0 {
                        i32::from(raw.x)
                    } else {
                        -i32::from(raw.x)
                    },
                    i32::from(raw.y),
                    i32::from(raw.z)
                ]
            );
        }
    }
}

#[test]
fn diagnostic_preserves_local_faces_while_omitting_runtime_model_instances() {
    let mut entry = diagnostic_entry(&[
        0x00d8, 7, 0, 0, 2, 4, // independently static shared-light triangle
        0x000e, 0x0036, 15, 6, // saved-basis child model, no state words
        0,
    ]);
    entry.header.vertex_ranges = [0, 0, 6, 6];
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("a runtime child must not hide independently proven parent geometry")
    };
    assert_eq!(geometry.faces.len(), 1);
    assert_eq!(
        geometry.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert_eq!(
        geometry
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
            .map(|omission| (omission.count, omission.opcodes.as_slice())),
        Some((1, [0x0e].as_slice()))
    );
    assert!(geometry
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::ExternalModelCatalog));
    assert!(geometry
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::SavedTransformBasis));
}

#[test]
fn diagnostic_keeps_pure_runtime_model_wrappers_unavailable() {
    let entry = diagnostic_entry(&[
        0x000e, 0x0036, 15, 6, // saved-basis child model, no local faces
        0,
    ]);
    let ModelDiagnosticProjection::Unavailable(unavailable) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("a pure runtime child wrapper has no honest static mesh")
    };
    assert_eq!(
        unavailable.kind,
        ModelDiagnosticUnavailableKind::ModelInstance
    );
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::ExternalModelCatalog));
    assert!(unavailable
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::SavedTransformBasis));
    assert_eq!(
        unavailable
            .omitted_effects
            .iter()
            .find(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
            .map(|omission| omission.count),
        Some(1)
    );
}

#[test]
fn diagnostic_mirror_pass_preserves_command_order_and_xors_slots() {
    let mut entry = diagnostic_entry(&[
        0x04d8, 7, 0, 0, 2, 4, // shared-light triangle plus mirror instance
        0,
    ]);
    entry.header.vertex_ranges = [0, 0, 6, 6];
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("static mirrored face must be projectable")
    };
    assert_eq!(geometry.faces.len(), 2);
    let face_slots = geometry
        .faces
        .iter()
        .map(|face| {
            face.iter()
                .map(|&index| geometry.vertices[index].slot.raw())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(face_slots, [vec![0, 2, 4], vec![1, 3, 5]]);
    assert!(!geometry.face_presentation[0].mirrored_instance);
    assert!(geometry.face_presentation[1].mirrored_instance);
    assert!(matches!(
        geometry.face_presentation[0].source_lighting,
        ModelDiagnosticFaceLighting::Shared(slot) if slot.raw() == 0
    ));
    assert!(matches!(
        geometry.face_presentation[1].source_lighting,
        ModelDiagnosticFaceLighting::Shared(slot) if slot.raw() == 1
    ));
}

#[test]
fn diagnostic_mirror_pass_xors_fan_lighting_without_flipping_uv_roles() {
    let mut entry = diagnostic_entry(&[
        1, 0x0018, 7, // textured fan plus mirror instance
        0, 8, 100, 200, 0x1111, 0, 2, 10, 300, 400, 0x2222, 0, 4, 12, 500, 600, 0x3333, 0, 2, 14,
        700, 800, 0x4444, 0x0100, 0,
    ]);
    entry.header.vertex_ranges = [0, 0, 6, 6];
    let ModelDiagnosticProjection::Ready(geometry) =
        analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(&entry))
    else {
        panic!("static mirrored fan must be projectable")
    };

    assert_eq!(geometry.faces.len(), 4);
    assert!(matches!(
        &geometry.face_presentation[0].source_lighting,
        ModelDiagnosticFaceLighting::PerVertex(slots)
            if slots.iter().map(|slot| slot.raw()).collect::<Vec<_>>() == [8, 10, 12]
    ));
    assert!(matches!(
        &geometry.face_presentation[1].source_lighting,
        ModelDiagnosticFaceLighting::PerVertex(slots)
            if slots.iter().map(|slot| slot.raw()).collect::<Vec<_>>() == [9, 11, 13]
    ));
    assert_eq!(
        geometry.face_presentation[0].fan_vertex_attributes,
        geometry.face_presentation[1].fan_vertex_attributes
    );
    assert_eq!(
        geometry
            .face_presentation
            .iter()
            .map(|presentation| presentation.fan_triangle_index)
            .collect::<Vec<_>>(),
        [Some(0), Some(0), Some(1), Some(1)]
    );
    assert_eq!(
        geometry
            .face_presentation
            .iter()
            .map(|presentation| presentation.mirrored_instance)
            .collect::<Vec<_>>(),
        [false, true, false, true]
    );
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_entry(payload: &mut Vec<u8>, name: Option<&str>, commands: &[u16]) {
    let flags = if name.is_some() { 0x20 } else { 0x40 };
    push_u32(payload, 321);
    push_u16(payload, commands.len() as u16);
    payload.push(1); // one four-byte auxiliary record
    payload.push(flags);
    push_u16(payload, 2); // runtime vertex slots
    push_u16(payload, 1); // raw XYZ records
    push_u16(payload, 4); // one raw normal: (4 - 2) / 2
    push_u16(payload, 0x1234); // nonzero enables the auxiliary consumer
    push_u16(payload, 0);
    for boundary in [0, 0, 2, 2] {
        push_u16(payload, boundary);
    }
    push_u16(payload, 0);
    push_u16(payload, 2);
    push_u16(payload, 4);
    push_u16(payload, 0x8887); // one-byte no-op, then auxiliary terminator
    push_u16(payload, 0x2222);
    for coordinate in [10i16, -20, 30] {
        payload.extend_from_slice(&coordinate.to_le_bytes());
    }
    for coordinate in [100i16, 200, -300] {
        payload.extend_from_slice(&coordinate.to_le_bytes());
    }
    for &word in commands {
        push_u16(payload, word);
    }
    if let Some(name) = name {
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
    }
    while payload.len() % 4 != 0 {
        payload.push(0xcc);
    }
}

fn synthetic_bank() -> (Vec<u8>, HandlerRegion, HandlerRequest, OverlayKey) {
    let mut payload = Vec::new();
    push_entry(&mut payload, Some("first"), &[0]);
    push_entry(&mut payload, None, &[0]);

    // Deliberately place the payload at absolute offset 5. Entries are aligned
    // relative to this payload, matching FUN_0050F5A0 rather than file offset.
    let mut source = vec![0xaa];
    push_u16(&mut source, payload.len() as u16);
    push_u16(&mut source, 2);
    source.extend_from_slice(&payload);
    let region = HandlerRegion {
        handler: HandlerId::MODELS,
        requested_count: 2,
        span: inf_formats::ByteSpan::new(1, source.len()),
        framing: HandlerFraming::ModelSubBlocks {
            total_entries: 2,
            blocks: vec![ModelSubBlock {
                span: inf_formats::ByteSpan::new(1, source.len()),
                entry_count: 2,
                payload: inf_formats::ByteSpan::new(5, source.len()),
            }],
        },
    };
    let request = HandlerRequest {
        count: 2,
        segments: vec![ResourceSegment {
            local_start: 0,
            global_delta: 5,
        }],
    };
    let overlay = OverlayKey {
        level: 7,
        variant: OverlayVariant::Base,
    };
    (source, region, request, overlay)
}

#[test]
fn parses_exact_entry_pools_names_alignment_and_global_identity() {
    let (source, region, request, overlay) = synthetic_bank();
    let bank = parse_model_bank(&source, &region, &request, &overlay).unwrap();

    assert_eq!(bank.overlay, overlay);
    assert_eq!(bank.blocks.len(), 1);
    assert_eq!(bank.entries.len(), 2);
    let first = &bank.entries[0];
    assert_eq!(first.identity.local_index, 0);
    assert_eq!(first.identity.global_index, 5);
    assert_eq!(first.name.as_ref().unwrap().ascii(&source), Some("first"));
    assert_eq!(first.auxiliary[0].words, [0x8887, 0x2222]);
    let auxiliary = first.auxiliary_program.as_ref().unwrap();
    assert_eq!(auxiliary.instructions[0].opcode, 0x87);
    assert_eq!(auxiliary.padding.get(&source).unwrap(), [0x22, 0x22]);
    assert_eq!(
        (
            first.vertices[0].x,
            first.vertices[0].y,
            first.vertices[0].z
        ),
        (10, -20, 30)
    );
    assert_eq!(
        (first.normals[0].x, first.normals[0].y, first.normals[0].z),
        (100, 200, -300)
    );
    assert_eq!(first.header.raw_normal_count(), 1);
    assert!(first.header.flags.forces_near_table());
    assert_eq!((first.span.start - bank.blocks[0].payload.start) % 4, 0);

    let second = &bank.entries[1];
    assert_eq!(second.identity.global_index, 6);
    assert!(second.header.flags.omits_name());
    assert!(second.name.is_none());
    assert_eq!((second.span.start - bank.blocks[0].payload.start) % 4, 0);
    assert_ne!(
        second.span.start % 4,
        0,
        "absolute file alignment is intentionally irrelevant"
    );
}

#[test]
fn auxiliary_admission_preserves_inactive_bytes_and_rejects_invalid_active_programs() {
    let (mut source, region, request, overlay) = synthetic_bank();
    let first_header = 5;
    let auxiliary_start = first_header + 0x20;
    source[auxiliary_start..auxiliary_start + 4].copy_from_slice(&[0x11, 0x11, 0x22, 0x22]);
    assert!(matches!(
        parse_model_bank(&source, &region, &request, &overlay),
        Err(ModelError::Auxiliary(inf_formats::ModelAuxiliaryError::UnknownOpcode {
            offset,
            opcode: 0x11,
        })) if offset == auxiliary_start
    ));
    source[first_header + 0x0e..first_header + 0x10].fill(0);
    let bank = parse_model_bank(&source, &region, &request, &overlay).unwrap();
    assert!(bank.entries[0].auxiliary_program.is_none());
    assert_eq!(bank.entries[0].auxiliary[0].words, [0x1111, 0x2222]);
    assert!(bank.entries[1].auxiliary_program.is_some());
}

#[test]
fn preserves_non_ascii_model_names_as_archive_bytes() {
    let (mut source, region, request, overlay) = synthetic_bank();
    let initial = parse_model_bank(&source, &region, &request, &overlay).unwrap();
    let name_start = initial.entries[0].name.as_ref().unwrap().bytes.start;
    source[name_start] = 0xe9;

    let bank = parse_model_bank(&source, &region, &request, &overlay).unwrap();
    let name = bank.entries[0].name.as_ref().unwrap();
    assert_eq!(name.raw(&source), Some(&b"\xe9irst"[..]));
    assert_eq!(name.ascii(&source), None);
}

#[test]
fn rejects_invalid_header_ranges_before_interpreting_pools() {
    let (mut source, region, request, overlay) = synthetic_bank();
    let first_header = 5;
    source[first_header + 0x0c..first_header + 0x0e].copy_from_slice(&3u16.to_le_bytes());
    assert!(matches!(
        parse_model_bank(&source, &region, &request, &overlay),
        Err(ModelError::InvalidShadeSlotCount { count: 3, .. })
    ));
}

#[test]
fn decodes_faces_fans_instances_nested_programs_and_proven_lengths() {
    let mut words = vec![
        0x0308, 0xaaaa, 0xbbbb, 0xcccc, // opcode 08: high byte is three words
        0x0004, 1, 2, 3, // opcode 04 has exactly three argument words
        0x0707, 9, 10, 2, 4, // edge
        0x07d0, 0xffff, 2, 4, 6, 8, 10, 12, // mirrored/cull per-vertex triangle
        0x0001, 0x0010, 7, // untextured mirrored fan prefix
        2, 8, 0x1111, 0x0000, 4, 10, 0x2222, 0x0000, 6, 12, 0x3333, 0x0100, 0x000e, 0x0036, 15, 8,
        0x2468, // instance with one state word
        0x000b, 1, 2, 3, // explicitly unresolved view-plane branch
        0x00f5, 1, 2, 3, // register operation
        0x0005,
    ];
    words.extend_from_slice(&[0; 12]);
    words.push(0); // nested terminator
    words.push(0); // root terminator

    let program = decode_model_program(&words).unwrap();
    assert_eq!(program.instructions.len(), 9);
    assert!(matches!(
        program.instructions[0].kind,
        ModelInstructionKind::VariableWords(_)
    ));
    assert!(matches!(
        program.instructions[1].kind,
        ModelInstructionKind::Opaque(_)
    ));
    assert!(matches!(
        program.instructions[2].kind,
        ModelInstructionKind::Edge(_)
    ));

    let ModelInstructionKind::Triangle(triangle) = &program.instructions[3].kind else {
        panic!("expected typed triangle")
    };
    assert_eq!(
        triangle.material.domain,
        ModelMaterialDomain::Handler7Material
    );
    assert_eq!(triangle.material.global_id, -1);
    assert_eq!(triangle.vertices.map(|slot| slot.raw()), [2, 4, 6]);
    assert!(triangle.flags.cull_enabled());
    assert!(triangle.flags.reverse_cull_direction());
    assert!(triangle.flags.emits_mirror_instance());
    assert_eq!(triangle.vertices[0].mirrored().raw(), 3);

    let ModelInstructionKind::TriangleFan(fan) = &program.instructions[4].kind else {
        panic!("expected typed fan")
    };
    assert_eq!(fan.material.domain, ModelMaterialDomain::Handler7Material);
    assert_eq!(fan.material.global_id, 7);
    assert_eq!(fan.vertices.len(), 3);
    assert!(fan.flags.emits_mirror_instance());
    assert!(!fan.flags.reverse_cull_direction());
    assert!(!fan.flags.culls_from_first_triangle());
    assert!(!fan.flags.culls_each_triangle());
    assert!(matches!(
        fan.vertices[0].attributes,
        ModelFanVertexAttributes::Untextured { .. }
    ));

    let ModelInstructionKind::ModelInstance(instance) = &program.instructions[5].kind else {
        panic!("expected typed model instance")
    };
    assert_eq!(instance.model.raw(), 15);
    assert_eq!(instance.state_words, [0x2468]);
    assert!(instance.orientation.uses_saved_basis());
    assert!(instance
        .dependencies
        .contains(&ModelDynamicDependency::ExternalModelCatalog));
    assert!(instance
        .dependencies
        .contains(&ModelDynamicDependency::SavedTransformBasis));

    let ModelInstructionKind::UnresolvedDynamic(dynamic) = &program.instructions[6].kind else {
        panic!("expected explicit unresolved dependency")
    };
    assert_eq!(dynamic.dependencies, [ModelDynamicDependency::ViewPlane]);
    assert!(matches!(
        program.instructions[7].kind,
        ModelInstructionKind::RegisterOperation(_)
    ));
    let ModelInstructionKind::NestedProgram(nested) = &program.instructions[8].kind else {
        panic!("expected nested program")
    };
    assert!(nested.program.instructions.is_empty());
}

#[test]
fn decodes_textured_fan_uv_words_without_inventing_coordinate_scale() {
    let words = [
        1, 0x0008, 4, 2, 8, 100, 200, 0x1111, 0, 4, 10, 300, 400, 0x2222, 0, 6, 12, 500, 600,
        0x3333, 0x0100, 0,
    ];
    let program = decode_model_program(&words).unwrap();
    let ModelInstructionKind::TriangleFan(fan) = &program.instructions[0].kind else {
        panic!("expected fan")
    };
    assert_eq!(fan.material.domain, ModelMaterialDomain::Handler3Texture);
    assert_eq!(fan.material.global_id, 4);
    assert!(fan.flags.has_textured_records());
    assert!(matches!(
        fan.vertices[2].attributes,
        ModelFanVertexAttributes::Textured {
            uv_words: [500, 600],
            ..
        }
    ));
}

#[test]
fn fan_flags_preserve_the_original_cull_modes() {
    let first_triangle = inf_formats::ModelFanFlags::from_raw(0x000b);
    assert!(first_triangle.reverse_cull_direction());
    assert!(first_triangle.culls_from_first_triangle());
    assert!(!first_triangle.culls_each_triangle());
    assert!(first_triangle.has_textured_records());

    let each_triangle = inf_formats::ModelFanFlags::from_raw(0x000c);
    assert!(!each_triangle.reverse_cull_direction());
    assert!(!each_triangle.culls_from_first_triangle());
    assert!(each_triangle.culls_each_triangle());
    assert!(each_triangle.has_textured_records());
}

#[test]
fn face_opcode_families_select_the_proven_runtime_handler_domain() {
    let cases: [(u8, ModelMaterialDomain, usize); 12] = [
        (0xd0, ModelMaterialDomain::Handler7Material, 7),
        (0xd2, ModelMaterialDomain::Handler3Texture, 7),
        (0xd4, ModelMaterialDomain::Handler7Material, 9),
        (0xd6, ModelMaterialDomain::Handler3Texture, 9),
        (0xd8, ModelMaterialDomain::Handler7Material, 5),
        (0xda, ModelMaterialDomain::Handler3Texture, 5),
        (0xdc, ModelMaterialDomain::Handler7Material, 5),
        (0xde, ModelMaterialDomain::Handler3Texture, 5),
        (0xe0, ModelMaterialDomain::Handler7Material, 6),
        (0xe2, ModelMaterialDomain::Handler3Texture, 6),
        (0xe4, ModelMaterialDomain::Handler7Material, 6),
        (0xe6, ModelMaterialDomain::Handler3Texture, 6),
    ];

    for (opcode, expected_domain, argument_count) in cases {
        let mut words = vec![u16::from(opcode), 0x1234];
        words.resize(1 + argument_count, 0);
        words.push(0);
        let program = decode_model_program(&words).unwrap();
        let material = match &program.instructions[0].kind {
            ModelInstructionKind::Triangle(face) => face.material,
            ModelInstructionKind::Quad(face) => face.material,
            other => panic!("opcode {opcode:#04x} decoded as {other:?}"),
        };
        assert_eq!(material.domain, expected_domain, "opcode {opcode:#04x}");
        assert_eq!(material.global_id, 0x1234, "opcode {opcode:#04x}");
    }
}

#[test]
fn rejects_unknown_or_unterminated_command_structures() {
    assert!(matches!(
        decode_model_program(&[0x000a, 0]),
        Err(ModelError::UnknownOpcode { opcode: 0x0a, .. })
    ));
    assert!(matches!(
        decode_model_program(&[0x0006, 1, 2, 0]),
        Err(ModelError::UnterminatedSentinelList { opcode: 0x06, .. })
    ));
    assert!(matches!(
        decode_model_program(&[0x000e, 0, 1, 7, 0]),
        Err(ModelError::InvalidInstanceLength { byte_len: 7, .. })
    ));
    assert!(matches!(
        decode_model_program(&[0x0004, 1, 2]),
        Err(ModelError::TruncatedCommand {
            opcode: 0x04,
            needed: 3,
            ..
        })
    ));
}

#[test]
fn exposes_exact_orientation_permutations_and_source_signs() {
    let expected = [
        Some([ModelAxis::X, ModelAxis::Y, ModelAxis::Z]),
        Some([ModelAxis::Z, ModelAxis::X, ModelAxis::Y]),
        Some([ModelAxis::Y, ModelAxis::Z, ModelAxis::X]),
        Some([ModelAxis::Z, ModelAxis::Y, ModelAxis::X]),
        Some([ModelAxis::Y, ModelAxis::X, ModelAxis::Z]),
        Some([ModelAxis::X, ModelAxis::Z, ModelAxis::Y]),
        None,
        Some([ModelAxis::X, ModelAxis::Y, ModelAxis::Z]),
    ];
    for (raw, mapping) in expected.into_iter().enumerate() {
        assert_eq!(
            inf_formats::ModelOrientation::from_raw(raw as u16).destination_sources(),
            mapping
        );
    }
    assert_eq!(
        inf_formats::ModelOrientation::from_raw(0x38).source_signs(),
        [-1, -1, -1]
    );
}

#[test]
fn every_retail_model_decodes_and_global_names_are_stable() {
    let root = infestation_root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut paths = fs::read_dir(root.join("Overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();

    let mut region_count = 0usize;
    let mut block_count = 0usize;
    let mut entry_count = 0usize;
    let mut diagnostic_projection_count = 0usize;
    let mut diagnostic_census = [0usize; 8];
    let mut nested_program_census = [0usize; 4];
    let mut dynamic_nested_globals = HashSet::new();
    let mut global_names = HashMap::<u16, String>::new();
    let mut global_ids = HashSet::new();
    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap();
        let key = OverlayKey::parse_filename(name).unwrap();
        let leaf = preload.leaf(key.level).unwrap();
        let requests = leaf.requests(key.request_kind());
        let request = requests.get(8).unwrap();
        if request.count == 0 {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 8)
            .unwrap();
        let bank = parse_model_bank(&bytes, region, request, &key)
            .unwrap_or_else(|error| panic!("failed to parse {name}: {error}"));
        region_count += 1;
        block_count += bank.blocks.len();
        entry_count += bank.entries.len();
        for entry in bank.entries {
            let dynamic_before = nested_program_census[2];
            count_nested_programs(&entry.program, &mut nested_program_census);
            if nested_program_census[2] != dynamic_before {
                dynamic_nested_globals.insert(entry.identity.global_index);
            }
            match analyze_model_diagnostic(inf_formats::ModelDiagnosticRequest::unconditioned(
                &entry,
            )) {
                ModelDiagnosticProjection::Ready(geometry) => {
                    diagnostic_projection_count += 1;
                    diagnostic_census[match geometry.completeness {
                        ModelDiagnosticCompleteness::CompleteMesh => 0,
                        ModelDiagnosticCompleteness::PartialMesh => 1,
                    }] += 1;
                }
                ModelDiagnosticProjection::Unavailable(unavailable) => {
                    if entry.identity.global_index == 168 {
                        // HEALTHSMALL's empty 05 body writes projections using
                        // callback-derived register 1. Its 120 faces occur in
                        // the root stream and consume those persistent writes.
                        assert_eq!(entry.program.instructions[0].raw_command, 0x00f5);
                        let ModelInstructionKind::NestedProgram(nested) =
                            &entry.program.instructions[1].kind
                        else {
                            panic!("HEALTHSMALL retains its transform program")
                        };
                        assert_eq!(entry.program.instructions[1].raw_command, 0x0005);
                        assert_eq!(nested.arguments[1..4], [0x00c1, 0, 0xc000]);
                        assert!(nested.program.instructions.is_empty());
                        assert_eq!(entry.program.instructions[2..].len(), 120);
                        assert!(entry.program.instructions[2..].iter().all(|instruction| {
                            matches!(instruction.kind, ModelInstructionKind::TriangleFan(_))
                        }));
                        assert_eq!(
                            unavailable.kind,
                            ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
                        );
                        assert_eq!(
                            unavailable.unresolved_dependencies,
                            [
                                ModelDynamicDependency::PackedValueCallback,
                                ModelDynamicDependency::RegisterFile
                            ]
                        );
                        assert_eq!(unavailable.omitted_effects.len(), 1);
                        assert_eq!(unavailable.omitted_effects[0].count, 120);
                        assert_eq!(
                            unavailable.omitted_effects[0].kind,
                            ModelDiagnosticOmissionKind::UnresolvedFaceSlots
                        );
                    }
                    diagnostic_census[match unavailable.kind {
                        ModelDiagnosticUnavailableKind::Empty => 2,
                        ModelDiagnosticUnavailableKind::NoPolygonGeometry => 3,
                        ModelDiagnosticUnavailableKind::RuntimeDependentGeometry => 4,
                        ModelDiagnosticUnavailableKind::NestedProgram => 5,
                        ModelDiagnosticUnavailableKind::ModelInstance => 6,
                        ModelDiagnosticUnavailableKind::UnresolvedCommandEffects => 7,
                    }] += 1;
                }
            }
            let [a, b, c, d] = entry.header.vertex_ranges;
            assert!(
                a <= b && b <= c && c <= d && d <= entry.header.vertex_slot_count,
                "{name}"
            );
            let [a, b] = entry.header.shade_ranges;
            assert!(a <= b && b <= entry.header.shade_slot_count, "{name}");
            let model_name = entry
                .name
                .expect("all retail model entries are named")
                .ascii(&bytes)
                .expect("retail model names are ASCII")
                .to_owned();
            global_ids.insert(entry.identity.global_index);
            if let Some(previous) =
                global_names.insert(entry.identity.global_index, model_name.clone())
            {
                assert_eq!(
                    previous, model_name,
                    "global model {}",
                    entry.identity.global_index
                );
            }
        }
    }

    assert_eq!(region_count, 36);
    assert_eq!(block_count, 457);
    assert_eq!(entry_count, 5_142);
    assert_eq!(diagnostic_projection_count, 4_134);
    assert_eq!(diagnostic_census, [2_369, 1_765, 11, 447, 290, 0, 260, 0]);
    // Handler 8's retail corpus contains only FUN_004ED9C0-style nested
    // transforms: 57 have literal rotations and are eligible for the static
    // projection, while global model 168 retains a live packed callback.
    assert_eq!(nested_program_census, [58, 57, 1, 0]);
    assert_eq!(dynamic_nested_globals, HashSet::from([168]));
    assert_eq!(global_ids, (0u16..760).collect::<HashSet<_>>());
    assert_eq!(global_names.len(), 760);
    assert_eq!(global_names.get(&0).map(String::as_str), Some("nothing"));
    assert_eq!(global_names.get(&1).map(String::as_str), Some("mark"));
    assert_eq!(global_names.get(&2).map(String::as_str), Some("beacon"));
    assert_eq!(global_names.get(&3).map(String::as_str), Some("missile"));
    assert_eq!(global_names.get(&4).map(String::as_str), Some("ammobox"));
    assert_eq!(global_names.get(&5).map(String::as_str), Some("powergen"));
}
