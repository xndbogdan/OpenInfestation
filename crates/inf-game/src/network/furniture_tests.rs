use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

struct Buckets(BTreeMap<usize, Vec<FurnitureRecord>>);

impl Buckets {
    fn from(value: &Value) -> Self {
        Self(
            value
                .as_object()
                .unwrap()
                .iter()
                .map(|(bucket, records)| {
                    let records = records
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| FurnitureRecord {
                            value: r[0].as_u64().unwrap() as u16,
                            state: r[1].as_u64().unwrap() as u8,
                            lookup: r[2].as_u64().unwrap() as u8,
                        })
                        .collect();
                    (bucket.parse().unwrap(), records)
                })
                .collect(),
        )
    }
    fn digest(&self) -> String {
        let mut bytes = Vec::new();
        for records in self.0.values() {
            for r in records {
                bytes.extend_from_slice(&r.value.to_le_bytes());
                bytes.extend_from_slice(&[r.state, r.lookup]);
            }
        }
        format!("{:x}", Sha256::digest(&bytes))
    }
}

impl FurnitureBuckets for Buckets {
    fn count(&self, bucket: usize) -> usize {
        self.0.get(&bucket).map_or(0, Vec::len)
    }
    fn record(&self, bucket: usize, index: usize) -> Option<FurnitureRecord> {
        self.0.get(&bucket)?.get(index).copied()
    }
    fn set_state(&mut self, bucket: usize, index: usize, state: u8) -> bool {
        self.0
            .get_mut(&bucket)
            .and_then(|r| r.get_mut(index))
            .map(|r| r.state = state)
            .is_some()
    }
    fn set_lookup(&mut self, bucket: usize, index: usize, lookup: u8) -> bool {
        self.0
            .get_mut(&bucket)
            .and_then(|r| r.get_mut(index))
            .map(|r| r.lookup = lookup)
            .is_some()
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// `check_net_furniture.py`: frames from 0045E0F0 one after another, each
/// fed to 0045E620 on another terrain.
#[test]
fn the_furniture_codec_matches_0045e0f0_and_0045e620() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/net_furniture_native.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let source = Buckets::from(&input["source"]);
        let mut target = Buckets::from(&input["target"]);
        let mut state = GameNetState::default();
        state.furniture_cursor = [0, 1].map(|i| input["cursor"][i].as_u64().unwrap() as u32);
        let host = input["host"].as_i64().unwrap() != 0;
        let mut received = case["received"].as_array().unwrap().iter();
        for frame in case["frames"].as_array().unwrap() {
            let mut out = [0xccu8; 0x200];
            let length = encode_furniture(
                &mut out,
                frame["room"].as_u64().unwrap() as u32,
                &mut state,
                &source,
            )
            .unwrap();
            assert_eq!(
                i64::from(length),
                frame["length"].as_i64().unwrap(),
                "{frame}"
            );
            assert_eq!(
                out[..0x88].to_vec(),
                hex(frame["out"].as_str().unwrap()),
                "{frame}"
            );
            let cursor: Vec<u64> = frame["cursor"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect();
            assert_eq!(
                state.furniture_cursor.map(u64::from).to_vec(),
                cursor,
                "{frame}"
            );
            if length < 4 {
                continue;
            }
            let expected = received.next().unwrap();
            let used = receive_furniture(&out, host, &mut target).unwrap();
            assert_eq!(
                i64::from(used),
                expected["used"].as_i64().unwrap(),
                "{frame}"
            );
            assert_eq!(
                target.digest(),
                expected["records"].as_str().unwrap(),
                "{frame}"
            );
        }
    }
}
