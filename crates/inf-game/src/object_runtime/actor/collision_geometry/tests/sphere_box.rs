use super::*;

fn word(v: &Value) -> u32 {
    v.as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| v.as_i64().unwrap() as u32)
}
fn vector(v: &Value) -> [u32; 3] {
    std::array::from_fn(|i| word(&v[i]))
}
fn normals(v: &Value) -> ContactAxisNormals {
    //Native fixture address order270,280,290,2B0,2C0,2D0.
    ContactAxisNormals {
        positive: [vector(&v[2]), vector(&v[1]), vector(&v[0])],
        negative: [vector(&v[4]), vector(&v[5]), vector(&v[3])],
    }
}

#[test]
fn combat_sphere_box_matches_all_native_faces_ties_wrapping_and_retained_fields() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/combat_auxiliary_sphere_box_native.json"
    ))
    .unwrap();
    assert_eq!(
        normals(&fixture["startup"]["normals"]),
        ContactAxisNormals::NATIVE_STARTUP
    );
    assert_eq!(fixture["startup"]["entries"].as_array().unwrap().len(), 6);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut contact = CollisionContact::from_native_words(std::array::from_fn(|i| {
            Some(word(&input["initial"][i]))
        }));
        apply_contact_geometry(
            &mut contact,
            ContactGeometryRequest::SphereBox {
                request: ContactSphere {
                    center: vector(&input["query"]).map(|v| v as i32),
                    radius: word(&input["radius"]) as i32,
                },
                center: vector(&input["center"]).map(|v| v as i32),
                half_extents: vector(&input["extents"]).map(|v| v as i32),
                normals: normals(&input["normals"]),
            },
        );
        assert_eq!(
            contact.native_words(),
            std::array::from_fn(|i| Some(word(&case["output"][i]))),
            "nativecase{index}"
        );
    }
}
