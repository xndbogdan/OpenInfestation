//!Native05 captures are consumed against actual retained H8 child geometry.
use super::*;
use crate::model_auxiliary::child::AuxiliaryChildRequest;
use serde_json::Value;

fn word(value: &Value) -> u32 {
    value
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| value.as_i64().unwrap() as u32)
}
fn basis(value: &Value) -> MenuBasis {
    MenuBasis {
        words: std::array::from_fn(|i| word(&value[i])),
    }
}
fn xyz(value: &Value) -> [i32; 3] {
    std::array::from_fn(|i| word(&value[i]) as i32)
}

#[test]
fn combat_child_uses_native_captured_basis_and_relative_query_with_retained_h8() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/combat_auxiliary_child_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    let runtime = resident();
    let child = source(&runtime, 223);
    let child_source = qualification(&child);
    let mut hits = 0;
    for (index, case) in cases.iter().enumerate() {
        if !case["normal_child_return"].as_bool().unwrap() {
            continue;
        }
        let input = &case["input"];
        let captured = &case["child"];
        let initial = CollisionContact::from_native_words(std::array::from_fn(|i| {
            Some(word(&captured["output_before"][i]))
        }));
        //The native receipt supplies its H8 getter result and recursive child
        //result. Replace only that supplied child with real resident H8/223,
        //then independently run that model with the native-captured operands.
        //This tests05 operand custody, not the receipt's synthetic addresses,
        //copied registers (still unowned), or its supplied recursive verdict.
        let (expected_owners, mut expected_process) = owners(true);
        let mut expected = initial;
        query_auxiliary_direct(
            AuxiliaryDirectRequest {
                model: AuxiliaryModelSource {
                    model: child.model,
                    qualification: &child_source,
                },
                basis: basis(&captured["basis"]),
                relative_center: std::array::from_fn(|i| word(&captured["query"][3 + i]) as i32),
                shape: AuxiliaryQueryShape::Sphere {
                    radius: word(&captured["query"][7]) as i32,
                },
                values: AuxiliaryValueSource::Required,
                residency: Some(&runtime),
            },
            &mut expected_process,
            &expected_owners,
            &mut expected,
        )
        .unwrap();

        let (owners, mut process) = owners(true);
        let scope = owners.scratch.begin_draw().unwrap();
        let allocation = owners.scratch.allocate(&scope, 32, 2).unwrap();
        let world = xyz(&input["world"]);
        let bytes: [u8; 12] = std::array::from_fn(|i| world[i / 4].to_le_bytes()[i % 4]);
        owners
            .scratch
            .write_vertex(
                &allocation,
                word(&input["attachment"]) as u16,
                0,
                bytes.map(Some),
            )
            .unwrap();
        let vertices = ModelVertices::in_scratch(&owners.scratch, allocation.clone()).unwrap();
        let mut parent = StackContext {
            origin: [0; 3],
            basis: basis(&input["basis"]),
            saved_basis: basis(&input["saved"]),
            table: ModelSurfaceTableOrigin::TemplateDefault,
        };
        let mut actual = initial;
        Run {
            graphics: &owners.graphics,
            scratch: &owners.scratch,
            scope: scope.clone(),
            process: &mut process,
            basis_math: &owners.basis_math,
            residency: Some(&runtime),
            terrain: None,
            counterpart: None,
            ray: None,
        }
        .execute_child(
            &Working::Stack(&mut parent),
            &vertices,
            AuxiliaryChildRequest {
                slot: 223,
                orientation: word(&input["orientation"]) as u16,
                attachment: word(&input["attachment"]) as i16,
                counterpart: AuxiliaryModelSource {
                    model: child.model,
                    qualification: &child_source,
                },
                relative: xyz(&input["relative"]),
                radius: word(&input["radius"]) as i32,
                callbacks: CallbackTable::Sphere544c08,
                values: AuxiliaryValueSource::Required,
            },
            &mut actual,
        )
        .unwrap();
        assert_eq!(
            actual.native_words(),
            expected.native_words(),
            "case{index}"
        );
        hits += usize::from(actual.flag() == Some(1));
        assert_eq!(
            vertices.world(word(&input["attachment"]) as i16).unwrap(),
            world
        );
        let publication = owners.graphics.world().unwrap();
        assert_eq!(
            publication.position,
            xyz(&case["publication"]),
            "case{index}"
        );
        assert_eq!(
            publication.basis.words,
            std::array::from_fn(|i| word(&case["publication"][3 + i])),
            "case{index}"
        );
        owners.scratch.retire(&scope, allocation.mark()).unwrap();
        owners
            .scratch
            .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
            .unwrap();
    }
    assert!(
        hits >= 2,
        "the two centered native requests exercise positive child contacts"
    );
}

#[test]
fn combat_child_actual_value_stop_preserves_child_publication_before_parent_restore() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/combat_auxiliary_child_native.json"
    ))
    .unwrap();
    let case = &fixture["cases"][11];
    assert_eq!(case["normal_child_return"], false);
    let input = &case["input"];
    let runtime = resident();
    let child = source(&runtime, 699);
    let qualification = qualification(&child);
    let (owners, mut process) = owners(true);
    let scope = owners.scratch.begin_draw().unwrap();
    let allocation = owners.scratch.allocate(&scope, 32, 2).unwrap();
    let world = xyz(&input["world"]);
    let bytes: [u8; 12] = std::array::from_fn(|i| world[i / 4].to_le_bytes()[i % 4]);
    owners
        .scratch
        .write_vertex(&allocation, 14, 0, bytes.map(Some))
        .unwrap();
    let vertices = ModelVertices::in_scratch(&owners.scratch, allocation.clone()).unwrap();
    let mut parent = StackContext {
        origin: [0; 3],
        basis: basis(&input["basis"]),
        saved_basis: basis(&input["saved"]),
        table: ModelSurfaceTableOrigin::TemplateDefault,
    };
    let mut contact = CollisionContact::unwritten();
    let result = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process: &mut process,
        basis_math: &owners.basis_math,
        residency: Some(&runtime),
        terrain: None,
        counterpart: None,
        ray: None,
    }
    .execute_child(
        &Working::Stack(&mut parent),
        &vertices,
        AuxiliaryChildRequest {
            slot: 699,
            orientation: 6,
            attachment: 14,
            counterpart: AuxiliaryModelSource {
                model: child.model,
                qualification: &qualification,
            },
            relative: xyz(&input["relative"]),
            radius: word(&input["radius"]) as i32,
            callbacks: CallbackTable::Sphere544c08,
            values: AuxiliaryValueSource::Required,
        },
        &mut contact,
    );
    assert!(
        matches!(result, Err(AuxiliaryModelError::RequiredPackedValue { .. })),
        "{result:?}"
    );
    assert_eq!(
        owners.graphics.world().unwrap().basis,
        basis(&case["child"]["basis"])
    );
    assert_ne!(owners.graphics.world().unwrap().basis, parent.basis);
    owners
        .scratch
        .finish_draw(&scope, ModelScratchDrawExit::StoppedOnError)
        .unwrap();
    assert!(matches!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
}
