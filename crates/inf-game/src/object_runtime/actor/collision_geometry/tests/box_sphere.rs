//! 004F15F0 against its original body under Unicorn
//! (`check_box_sphere.py`).
use super::*;

#[test]
fn box_sphere_matches_every_native_face_tie_and_wrap() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/combat_auxiliary_box_sphere_native.json"
    ))
    .unwrap();
    // 0055F270 +Z, 280 +Y, 290 +X, 2B0 -Z, 2C0 -X, 2D0 -Y as supplied.
    let normal = |address: &str| -> [u32; 3] {
        std::array::from_fn(|i| fixture["normals"][address][i].as_u64().unwrap() as u32)
    };
    assert_eq!(
        ContactAxisNormals {
            positive: [normal("0055F290"), normal("0055F280"), normal("0055F270")],
            negative: [normal("0055F2C0"), normal("0055F2D0"), normal("0055F2B0")],
        },
        ContactAxisNormals::NATIVE_STARTUP
    );
    let poison: [i32; 5] = std::array::from_fn(|i| dword(&fixture["poison"][i]));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 207);
    let mut hits = 0;
    for case in cases {
        let input = &case["input"];
        let mut contact = CollisionContact::from_native_words(poison.map(|word| Some(word as u32)));
        apply_contact_geometry(
            &mut contact,
            ContactGeometryRequest::BoxSphere {
                center: xyz(&input["center"]),
                half_extents: xyz(&input["extents"]),
                sphere: ContactSphere {
                    center: xyz(&input["query"]),
                    radius: dword(&input["radius"]),
                },
                normals: ContactAxisNormals::NATIVE_STARTUP,
            },
        );
        let expected: [i32; 5] = std::array::from_fn(|i| dword(&case["output"][i]));
        assert_eq!(
            contact.native_words().map(|word| word.unwrap() as i32),
            expected,
            "{input}"
        );
        hits += usize::from(expected[0] == 1);
    }
    assert_eq!(hits, 69);
}
