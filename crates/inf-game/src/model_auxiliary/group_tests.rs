use super::*;

/// The normal is the first edge crossed with the second: x across z is -y.
#[test]
fn a_triangle_reaching_the_sphere_is_a_hit() {
    let vertices = [[0, 0, 0], [100, 0, 0], [0, 0, 100]];
    assert_eq!(
        plane(vertices, [10, 30, 10], 40),
        PlaneHit::Hit {
            penetration: 70,
            normal: [0, (-1.0f32).to_bits(), 0],
        }
    );
    assert_eq!(plane(vertices, [10, -60, 10], 40), PlaneHit::Miss);
}

/// Every case of `auxiliary_groups_native.json`: the original 004F3B20,
/// 004F3C60, 004F2720, 004F2930, 004F2710, 004F2730 and 0044FBE0 under
/// emulation, the first ones Infest03's recorded calls. The result's five
/// DWORDs after the port's group test match each one.
#[test]
fn group_tests_match_the_native_receipt() {
    use crate::model_auxiliary::{group_words, CallbackTable};
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../tests/fixtures/auxiliary_groups_native.json"
    ))
    .unwrap();
    let int = |value: &serde_json::Value| value.as_i64().unwrap() as i32;
    let triple = |value: &serde_json::Value| -> [i32; 3] {
        let items = value.as_array().unwrap();
        [int(&items[0]), int(&items[1]), int(&items[2])]
    };
    let words = |value: &serde_json::Value| -> [Option<u32>; 5] {
        std::array::from_fn(|i| Some(value[i].as_u64().unwrap() as u32))
    };
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(fixture["recorded"].as_u64(), Some(159));
    let mut hits = 0;
    for (index, case) in cases.iter().enumerate() {
        let table = match case["table"].as_str().unwrap() {
            "ray" => CallbackTable::Ray544c20,
            "sphere" => CallbackTable::Sphere544c08,
            "point" => CallbackTable::Point544bf0,
            "terrain" => CallbackTable::Terrain537090,
            other => panic!("unknown table {other}"),
        };
        let vertices: Vec<[i32; 3]> = case["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .map(triple)
            .collect();
        let after = group_words(
            table,
            case["offset"].as_u64().unwrap() as u8,
            &vertices,
            triple(&case["relative"]),
            int(&case["radius"]),
            Some(int(&case["length"])),
            words(&case["before"]),
        )
        .unwrap();
        assert_eq!(after, words(&case["after"]), "case {index}: {case}");
        hits += usize::from(after[0] == Some(1));
    }
    assert!(hits > 600, "{hits} hits");
}
