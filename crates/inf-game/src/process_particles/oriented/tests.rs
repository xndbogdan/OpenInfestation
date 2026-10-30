use super::*;
use serde_json::{json, Value};

pub(super) fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
fn u(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}
fn words(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&value[i]) as u16)
}

#[test]
fn pickup_oriented_emission_matches_native_branch_rng_and_birth_prefixes() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/pickup_lifecycle_native.json"
    ))
    .unwrap();
    let raw = executable_bytes();
    let exe = FrontendExecutable::parse(&raw).unwrap();
    let bytes = exe.mapped(0x0053_df00, 8192).unwrap().bytes;
    let sine = std::array::from_fn(|i| u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
    let mut count = 0;
    for row in data["trails"].as_array().unwrap() {
        let input = &row["input"];
        let position = words(&input["position"]);
        let velocity = words(&input["velocity"]);
        let basis = NativeBasis {
            words: std::array::from_fn(|i| u(&input["basis"][i])),
        };
        let request = OrientedParticleRequest {
            position: &position,
            velocity: &velocity,
            basis: &basis,
            strength: u(&input["strength"]) as i32,
            load_factor: u(&input["load"]) as i32,
            plane: u(&input["plane"]) as i16,
            sine_quarter: &sine,
        };
        let mut rng = u(&input["rng"]);
        let mut events = Vec::new();
        let result = emit(
            &request,
            || {
                Ok::<_, ()>(f32::from_le_bytes(
                    exe.mapped(0x0052_0a64, 4)
                        .unwrap()
                        .bytes
                        .try_into()
                        .unwrap(),
                ))
            },
            &mut rng,
            |kind, position, velocity, rng| {
                events.push(json!({"address":match kind {Emission::Bubble=>0x489600,Emission::FallingDrop=>0x489920},
                    "position":position,"velocity":velocity,"rng":*rng}));
                if input["stop"].as_u64() == Some(events.len() as u64) {
                    Err(())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(Value::Array(events), row["events"], "input {input}");
        assert_eq!(rng, u(&row["rng"]), "input {input}");
        assert_eq!(result.is_ok(), row["normal_return"].as_bool().unwrap());
        count += 1;
    }
    assert_eq!(count, 13);
}
