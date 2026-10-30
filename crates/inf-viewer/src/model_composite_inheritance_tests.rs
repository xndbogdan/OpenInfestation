use super::*;
use inf_formats::{ByteSpan, ModelInstruction, ModelUnresolvedCommand};
use std::path::Path;

fn operation(opcode: u16, arguments: &[u16]) -> ModelInstruction {
    ModelInstruction {
        raw_command: opcode,
        span: ByteSpan::new(0, 8),
        kind: ModelInstructionKind::RegisterOperation(ModelUnresolvedCommand {
            arguments: arguments.to_vec(),
            dependencies: vec![ModelDynamicDependency::RegisterFile],
        }),
    }
}

#[test]
fn child_registers_inherit_only_four_words_with_wrapping_and_neutral_provenance() {
    let mut parent = NeutralRegisterFile::default();
    // Literal !0 - 0, then a one-bit shift: retain native WORD truncation.
    parent.apply(&operation(0xf1, &[0, 0x40, 0]));
    parent.apply(&operation(0xf5, &[1, 0xc0, 1]));
    // Callback-backed zero minus literal one remains callback-backed.
    parent.apply(&operation(0xf1, &[2, 0x80, 1]));
    // Register 3 intentionally remains unknown; 4 and 21 must not inherit.
    parent.apply(&operation(0xf1, &[4, 3, 1]));
    parent.apply(&operation(0xf1, &[21, 3, 1]));
    let child = parent.for_child();
    assert_eq!(
        child.resolve(0xc0),
        Some(NeutralValue {
            raw: u16::MAX,
            callback_backed: false,
        })
    );
    assert_eq!(
        child.resolve(0xc1),
        Some(NeutralValue {
            raw: 0xfffe,
            callback_backed: false,
        })
    );
    assert_eq!(
        child.resolve(0xc2),
        Some(NeutralValue {
            raw: u16::MAX,
            callback_backed: true,
        })
    );
    assert!(parent.resolve(0xc4).is_some());
    assert!(parent.resolve(0xd5).is_some());
    for register in 3..64 {
        assert_eq!(child.resolve(0xc0 | register), None);
    }
}

#[test]
fn child_writes_reach_its_descendants_without_changing_parent_or_siblings() {
    let mut parent = NeutralRegisterFile::default();
    parent.apply(&operation(0xf1, &[1, 3, 1]));
    let mut child = parent.for_child();
    child.apply(&operation(0xf1, &[1, 0xc1, 3]));
    assert_eq!(child.resolve(0xc1).unwrap().raw, u16::MAX);
    assert_eq!(child.for_child().resolve(0xc1).unwrap().raw, u16::MAX);
    assert_eq!(parent.resolve(0xc1).unwrap().raw, 2);
    assert_eq!(parent.for_child().resolve(0xc1).unwrap().raw, 2);

    // Each later sibling sees the parent's value at its own call boundary.
    parent.apply(&operation(0xf5, &[1, 0xc1, 2]));
    assert_eq!(parent.for_child().resolve(0xc1).unwrap().raw, 8);
    assert_eq!(child.resolve(0xc1).unwrap().raw, u16::MAX);
}

#[test]
fn retail_titles_inherit_neutral_clock_values_into_planet_children() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let catalog = AssetCatalog::load(&root, None).unwrap();
    let titles = catalog
        .active_model_variant_in_source(738, "47.ovl")
        .expect("retail titles parent");
    let composite = build_neutral_model_composite(&catalog, titles, "47.ovl")
        .expect("titles composes its five inherited-angle planets and Jupiter's rings");
    // Without register inheritance only the seven immediate children appear,
    // totaling 35 vertices / 20 faces; the planet wrappers cannot resolve C1.
    assert_eq!(composite.child_instances, 13);
    assert_eq!(composite.geometry.vertices.len(), 1374);
    assert_eq!(composite.geometry.faces.len(), 2664);
    assert_eq!(
        composite.geometry.completeness,
        ModelDiagnosticCompleteness::PartialMesh
    );
    assert!(composite
        .geometry
        .unresolved_dependencies
        .contains(&ModelDynamicDependency::PackedValueCallback));

    // Opening an inherited-angle wrapper as a root supplies no parent state.
    // Its missing C1 must remain unknown, rather than becoming literal zero.
    for wrapper_id in 739..=743 {
        let wrapper = catalog
            .active_model_variant_in_source(wrapper_id, "47.ovl")
            .unwrap();
        assert!(build_neutral_model_composite(&catalog, wrapper, "47.ovl").is_none());
    }
}
