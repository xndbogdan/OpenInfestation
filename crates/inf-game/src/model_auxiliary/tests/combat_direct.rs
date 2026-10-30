use super::*;
use serde_json::Value;

fn word(value: &Value) -> u32 {
    value
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| value.as_i64().unwrap() as u32)
}

#[test]
fn combat_direct_point_and_sphere_callbacks_match_native_contact_words() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/combat_direct_sphere_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 13);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut contact = CollisionContact::from_native_words(std::array::from_fn(|i| {
            Some(word(&input["initial"][i]))
        }));
        //004F1400 writes zero radius before the same4F1420 primitive. Retaining
        //the supplied radius999 would make the outside/tangent cases fail.
        let radius = if input["point"].as_bool().unwrap() {
            0
        } else {
            word(&input["query_radius"]) as i32
        };
        assert_eq!(radius as u32, word(&case["query_radius_after"]));
        apply_contact_geometry(
            &mut contact,
            ContactGeometryRequest::SpherePair {
                request: ContactSphere {
                    center: std::array::from_fn(|i| word(&input["query"][i]) as i32),
                    radius,
                },
                candidate: ContactSphere {
                    center: std::array::from_fn(|i| word(&input["center"][i]) as i32),
                    radius: word(&input["model_radius"]) as i32,
                },
            },
        );
        assert_eq!(
            contact.native_words(),
            std::array::from_fn(|i| Some(word(&case["output"][i]))),
            "native callback case{index}"
        );
    }
}

#[test]
fn combat_actual_direct_entry_selects_point_and_sphere_over_retained_h8_model() {
    let runtime = resident();
    let model = source(&runtime, 223);
    let qualification = qualification(&model);
    for (shape, depth) in [
        (AuxiliaryQueryShape::Point, 48),
        (AuxiliaryQueryShape::Sphere { radius: 4 }, 52),
    ] {
        let (owners, mut process) = owners(true);
        let mut contact = CollisionContact::from_native_words([Some(9); 5]);
        query_auxiliary_direct(
            AuxiliaryDirectRequest {
                residency: Some(&runtime),
                values: AuxiliaryValueSource::Required,
                model: AuxiliaryModelSource {
                    model: model.model,
                    qualification: &qualification,
                },
                basis: MenuBasis::INITIAL,
                relative_center: [0, 60, 0],
                shape,
            },
            &mut process,
            &owners,
            &mut contact,
        )
        .unwrap();
        assert_eq!(contact.flag(), Some(1));
        assert_eq!(contact.displacement(), Some(depth));
        assert_eq!(
            contact.normal_bits(),
            [Some(1.0f32.to_bits()), Some(0), Some(0)]
        );
        assert_eq!(
            owners.scratch.state().unwrap(),
            ModelScratchDrawState::Ready
        );
        assert_eq!(owners.graphics.world().unwrap().basis, MenuBasis::INITIAL);
    }
}
