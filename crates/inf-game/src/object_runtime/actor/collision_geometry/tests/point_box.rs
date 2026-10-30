//! 004F1D20 against its original body under Unicorn
//! (`check_point_box.py`).
use super::*;

#[test]
fn point_box_matches_every_native_gate_face_tie_and_wrap() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/combat_auxiliary_point_box_native.json"
    ))
    .unwrap();
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
    assert_eq!(cases.len(), 209);
    let mut hits = 0;
    for case in cases {
        let input = &case["input"];
        let mut contact = CollisionContact::from_native_words(poison.map(|word| Some(word as u32)));
        apply_contact_geometry(
            &mut contact,
            ContactGeometryRequest::PointBox {
                point: xyz(&input["point"]),
                center: xyz(&input["center"]),
                half_extents: xyz(&input["extents"]),
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
    assert_eq!(hits, 41);
}
