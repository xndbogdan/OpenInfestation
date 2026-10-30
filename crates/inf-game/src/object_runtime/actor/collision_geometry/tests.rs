use super::*;
use serde_json::Value;
mod box_box;
mod box_sphere;
mod point_box;
mod sphere_box;
mod terrain_sphere;

fn dword(value: &Value) -> i32 {
    value.as_u64().unwrap() as u32 as i32
}

fn xyz(value: &Value) -> [i32; 3] {
    std::array::from_fn(|i| dword(&value[i]))
}

fn words(value: &Value) -> [Option<u32>; 5] {
    std::array::from_fn(|i| value[i].as_u64().map(|word| word as u32))
}

fn request(step: &Value) -> ContactGeometryRequest {
    match step["kind"].as_str().unwrap() {
        "sphere_pair" => ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: xyz(&step["request_center"]),
                radius: dword(&step["request_radius"]),
            },
            candidate: ContactSphere {
                center: xyz(&step["candidate_center"]),
                radius: dword(&step["candidate_radius"]),
            },
        },
        "group_direction" => ContactGeometryRequest::GroupDirection {
            relative_xyz: xyz(&step["relative_xyz"]),
        },
        kind => panic!("unknown native contact fixture kind {kind}"),
    }
}

#[test]
fn supplied_contact_sequences_match_unchanged_native_bytes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/contact_geometry_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["fpcw"].as_u64(), Some(0x027f));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 540);
    let mut sphere_hits = 0;
    let mut sphere_rejections = 0;
    let mut group_count = 0;
    let mut discriminated_divisor = false;
    for case in cases {
        let mut contact = CollisionContact::from_native_words(words(&case["initial_words"]));
        let steps = case["steps"].as_array().unwrap();
        for (index, step) in steps.iter().enumerate() {
            let before = contact;
            apply_contact_geometry(&mut contact, request(step));
            assert_eq!(
                contact.native_words(),
                words(&step["final_words"]),
                "{} step {index}",
                case["name"]
            );
            if step["kind"] == "sphere_pair" {
                if step["writes"].as_array().unwrap().is_empty() {
                    sphere_rejections += 1;
                    assert_eq!(contact, before, "rejection must preserve every DWORD");
                } else {
                    sphere_hits += 1;
                    assert_eq!(contact.flag(), Some(1));
                }
            } else {
                group_count += 1;
                assert_eq!(contact.flag(), Some(1));
                assert_eq!(contact.displacement(), Some(1));
            }
        }
        if case["name"]
            .as_str()
            .unwrap()
            .starts_with("paired-denominator-")
        {
            discriminated_divisor |=
                words(&steps[0]["final_words"])[1..4] != words(&steps[1]["final_words"])[1..4];
        }
    }
    assert_eq!(
        (sphere_hits, sphere_rejections, group_count),
        (162, 211, 175)
    );
    assert!(
        discriminated_divisor,
        "fixture must distinguish the f32 divisor spill"
    );
}

#[test]
fn rejection_does_not_own_unknown_normal_or_displacement() {
    let mut contact = CollisionContact::unwritten();
    apply_contact_geometry(
        &mut contact,
        ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: [1, 0, 0],
                radius: 0,
            },
            candidate: ContactSphere {
                center: [0; 3],
                radius: 0,
            },
        },
    );
    assert_eq!(contact.native_words(), [None; 5]);
    assert_eq!(contact.flag(), None);
    assert_eq!(contact.normal_bits(), [None; 3]);
    assert_eq!(contact.displacement(), None);
    let partial = [Some(1), None, Some(0x80000000), None, Some(0xffffffff)];
    contact = CollisionContact::from_native_words(partial);
    apply_contact_geometry(
        &mut contact,
        ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: [9, 9, 9],
                radius: 1,
            },
            candidate: ContactSphere {
                center: [0; 3],
                radius: 0,
            },
        },
    );
    assert_eq!(contact.native_words(), partial);
    assert_eq!(contact.displacement(), Some(-1));
}

#[test]
fn lowword_normals_keep_full_dword_gates_and_unsigned_squared_distance() {
    let mut contact = CollisionContact::unwritten();
    apply_contact_geometry(
        &mut contact,
        ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: [40000, 30000, 0],
                radius: 50000,
            },
            candidate: ContactSphere {
                center: [0; 3],
                radius: 0,
            },
        },
    );
    // 2_500_000_000 is admitted unsigned, while LOWWORD(40000) is -25536.
    assert_eq!(contact.flag(), Some(1));
    assert_eq!(contact.displacement(), Some(0));
    assert_eq!(
        contact.normal_bits(),
        [
            Some((-25536.0_f32 / 50000.0).to_bits()),
            Some(0.6_f32.to_bits()),
            Some(0)
        ]
    );
    apply_contact_geometry(
        &mut contact,
        ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: [65536, 0, 0],
                radius: 65536,
            },
            candidate: ContactSphere {
                center: [0; 3],
                radius: 0,
            },
        },
    );
    // Wrapped square is zero, so native zero-distance fallback wins.
    assert_eq!(
        contact.normal_bits(),
        [Some(1.0_f32.to_bits()), Some(0), Some(0)]
    );
    assert_eq!(contact.displacement(), Some(65536));
    apply_contact_geometry(
        &mut contact,
        ContactGeometryRequest::GroupDirection {
            relative_xyz: [65536, 0, 0],
        },
    );
    assert_eq!(
        contact.normal_bits(),
        [Some(1.0_f32.to_bits()), Some(0), Some(0)]
    );
    assert_eq!(contact.displacement(), Some(1));
}
