//! Complete native rotating-base routines, with seeded prior state explicitly
//! separate from native construction and without actor-construction claims.

use std::path::Path;

use inf_formats::native_basis::{NativeBasis, NativeBasisMath, RotationPlane};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        native_model_fixture::{Fixture, Models},
        ObjectClassKind, ObjectList, ObjectNodeKind,
    },
};

fn snapshot(arena: &ObjectArena, object: ObjectId) -> Value {
    let state = arena.object(object).unwrap();
    let ObjectClass::RotatingVisual(rotating) = &state.class else {
        panic!("rotating visual class");
    };
    let base = &state.base;
    let mut lists = serde_json::Map::new();
    for (name, role) in [
        ("flag0", ObjectList::Flag0Primary),
        ("flag1", ObjectList::Flag1Primary),
        ("flag0_secondary", ObjectList::Flag0Secondary),
        ("flag1_secondary", ObjectList::Flag1Secondary),
        ("parking", ObjectList::Parking),
        ("disposal", ObjectList::Disposal),
    ] {
        let offsets: Vec<_> = arena
            .list(role)
            .iter()
            .map(|node| {
                assert_eq!(node.object, object);
                match node.kind {
                    ObjectNodeKind::Flag0 => 4,
                    ObjectNodeKind::Flag1 => 16,
                    ObjectNodeKind::Lifecycle => 28,
                }
            })
            .collect();
        lists.insert(name.to_owned(), json!(offsets));
    }
    json!({
        "word_28":base.word_28,"placement_word":base.placement_word,"flags":base.flags,
        "resource_word":base.resource_word,"position_words":base.position_words,
        "dirty_word":base.dirty_word,"word_3c":base.word_3c,
        "word_40":rotating.visual.radius_word,"model_word":rotating.visual.model_word,
        "basis_words":rotating.basis.words,"common_lists":lists,
    })
}

#[test]
fn rotating_context_angles_and_word_transforms_match_native_receipts() {
    let receipts: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/object_rotating_native.json"
    ))
    .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let math = NativeBasisMath::new(FrontendAssets::open(&root).unwrap().sine_table().values);
    let cases = receipts["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 75);
    let mut compared = 0;
    let mut transforms = 0;
    for case in cases {
        let models: Models = serde_json::from_value(case["h8"].clone()).unwrap();
        let fixture = Fixture::new(&models);
        let mut arena = ObjectArena::new();
        let object = arena.allocate(ObjectClassKind::RotatingVisualBase);
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            match step["op"].as_str().unwrap() {
                "construct" => {}
                "seed" => {
                    let state = &mut arena.owned_mut(object).unwrap().state;
                    if let Some(value) = step["dirty_word"].as_u64() {
                        state.base.dirty_word = value as u16;
                    }
                    if let Some(words) = step.get("basis_words") {
                        let ObjectClass::RotatingVisual(rotating) = &mut state.class else {
                            unreachable!()
                        };
                        rotating.basis = NativeBasis {
                            words: serde_json::from_value(words.clone()).unwrap(),
                        };
                    }
                }
                "context" => arena
                    .initialize_rotating_visual_context(
                        object,
                        RotatingVisualContext {
                            base: BaseContext {
                                position_words: serde_json::from_value(
                                    step["position_words"].clone(),
                                )
                                .unwrap(),
                                placement_index: step["index_dword"].as_u64().unwrap() as u32,
                            },
                            model_argument: step["model_argument"]
                                .as_u64()
                                .map(|value| value as u32),
                            rotation_words: serde_json::from_value(step["rotation_words"].clone())
                                .unwrap(),
                        },
                        &fixture.runtime,
                        &math,
                    )
                    .unwrap(),
                op @ ("rotate_x" | "rotate_y" | "rotate_z") => {
                    let plane = match op {
                        "rotate_x" => RotationPlane::Yz,
                        "rotate_y" => RotationPlane::Xz,
                        _ => RotationPlane::Xy,
                    };
                    arena
                        .rotate_visual(
                            object,
                            plane,
                            step["angle_dword"].as_u64().unwrap() as u32 as i32,
                            &math,
                        )
                        .unwrap();
                }
                "transform" => {
                    let input = serde_json::from_value(step["input_words"].clone()).unwrap();
                    let result = arena.transform_visual_vector_words(object, input).unwrap();
                    assert_eq!(
                        json!(result),
                        step["output_words"],
                        "{} transform",
                        case["id"]
                    );
                    transforms += 1;
                }
                op => panic!("unknown native operation {op}"),
            }
            assert_eq!(
                snapshot(&arena, object),
                step["state"],
                "{} step {index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 219);
    assert_eq!(transforms, 14);
}
